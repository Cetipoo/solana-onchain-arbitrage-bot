//! Batched, memoized account reads for building each mint's pools.
use crate::pump_fees::FeeRecipients;
use crate::v10::{self as abi, MintAccounts, PoolAccounts};
use crate::{arrays, clmm};
use anyhow::{ensure, Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::Account,
    pubkey::Pubkey,
    sysvar::clock::{self, Clock},
};
use std::collections::{BTreeSet, HashMap};

/// Fetches accounts in batches and memoizes them for one load, so a mint's
/// pools cost a few round trips instead of one request per account.
pub struct AccountLoader<'a> {
    rpc: &'a RpcClient,
    accounts: HashMap<Pubkey, Option<Account>>,
}

impl<'a> AccountLoader<'a> {
    pub fn new(rpc: &'a RpcClient) -> Self {
        Self {
            rpc,
            accounts: HashMap::new(),
        }
    }

    pub fn prefetch(&mut self, keys: impl IntoIterator<Item = Pubkey>) -> Result<()> {
        let missing: Vec<Pubkey> = keys
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|key| !self.accounts.contains_key(key))
            .collect();
        for chunk in missing.chunks(100) {
            let loaded = self.rpc.get_multiple_accounts(chunk)?;
            ensure!(loaded.len() == chunk.len(), "incomplete account response");
            self.accounts.extend(chunk.iter().copied().zip(loaded));
        }
        Ok(())
    }

    /// Fetches the pools and then, in one more round trip, every account
    /// their builders read and the arrays their walks reach with the bitmap
    /// extensions fetched so far. A third, only when a newly fetched
    /// extension lengthens a walk, fetches the arrays the walks reach beyond
    /// it. A pool that is missing or unreadable is left for `load_pool` to
    /// report.
    pub fn prefetch_pools(&mut self, pools: impl IntoIterator<Item = Pubkey>) -> Result<()> {
        let pools: Vec<Pubkey> = pools.into_iter().collect();
        self.prefetch(pools.iter().copied())?;
        let mut reads = Vec::new();
        for pool in &pools {
            if let Some(state) = self.accounts[pool].as_ref() {
                reads.extend(pool_reads(*pool, state).unwrap_or_default());
            }
        }
        reads.extend(self.walked_arrays(&pools));
        self.prefetch(reads)?;
        let arrays = self.walked_arrays(&pools);
        self.prefetch(arrays)
    }

    /// The arrays the fetched `pools`' walks reach with the bitmap extensions
    /// fetched so far. A CLMM walk before its extension is fetched is a
    /// prefix of the walk the extension steers.
    fn walked_arrays(&self, pools: &[Pubkey]) -> Vec<Pubkey> {
        pools
            .iter()
            .filter_map(|pool| {
                let state = self.accounts[pool].as_ref()?;
                self.walks(*pool, state).ok()
            })
            .flat_map(|walks| walks.concat())
            .collect()
    }

    /// `pool`'s walks over the accounts fetched so far.
    fn walks(&self, pool: Pubkey, state: &Account) -> Result<[Vec<Pubkey>; 2]> {
        arrays::array_walks(pool, state, |key| self.fetched(key))
    }

    /// The account at `key`, if it was fetched and exists.
    fn fetched(&self, key: &Pubkey) -> Option<&Account> {
        self.accounts.get(key).and_then(Option::as_ref)
    }

    /// Whether `key` was fetched and exists under `owner`.
    fn exists(&self, key: &Pubkey, owner: Pubkey) -> bool {
        self.fetched(key)
            .is_some_and(|account| account.owner == owner)
    }

    fn account(&mut self, key: Pubkey) -> Result<Option<&Account>> {
        self.prefetch([key])?;
        Ok(self.accounts[&key].as_ref())
    }

    /// Prefetched accounts, `None` where one does not exist.
    fn cached(&self, keys: &[Pubkey]) -> Vec<Option<Account>> {
        keys.iter().map(|key| self.accounts[key].clone()).collect()
    }

    pub fn get(&mut self, key: Pubkey) -> Result<Account> {
        self.account(key)?
            .cloned()
            .with_context(|| format!("account {key} not found"))
    }

    pub fn mint(&mut self, wallet: Pubkey, mint: Pubkey) -> Result<MintAccounts> {
        let account = self.get(mint)?;
        MintAccounts::ata(&wallet, mint, account.owner)
    }

    /// A direct pool, trading the target against whichever settlement mint
    /// the pool quotes it in.
    pub fn load_direct_pool(
        &mut self,
        pool: Pubkey,
        target: MintAccounts,
        wallet: Pubkey,
    ) -> Result<PoolAccounts> {
        let state = self
            .get(pool)
            .with_context(|| format!("load pool {pool}"))?;
        let pair = abi::PoolPair::read(state.owner, &state.data)?;
        let quote = *pair
            .mints
            .iter()
            .find(|mint| **mint != target.mint)
            .context("direct pool does not trade the target")?;
        ensure!(
            pair.mints.contains(&target.mint) && [abi::SOL, abi::USDC].contains(&quote),
            "direct pool {pool} does not pair the target with SOL or USDC"
        );
        let quote = self.mint(wallet, quote)?;
        self.load_pool(pool, target, quote, wallet)
    }

    pub fn load_pool(
        &mut self,
        pool: Pubkey,
        x: MintAccounts,
        base: MintAccounts,
        wallet: Pubkey,
    ) -> Result<PoolAccounts> {
        let state = self
            .get(pool)
            .with_context(|| format!("load pool {pool}"))?;
        let reads = pool_reads(pool, &state)?;
        self.prefetch(reads.iter().copied())?;
        match state.owner {
            abi::PUMP => {
                let global = self.get(reads[0])?;
                ensure!(global.owner == abi::PUMP, "invalid Pump global owner");
                let mut keys = abi::PumpKeys::from_state(&state.data, &global.data, x, base)?;
                let quote = keys.quote;
                let mut recipients = FeeRecipients::new(
                    &global.data,
                    state.data.get(243) == Some(&1),
                    quote.mint,
                    quote.token_program,
                )?;
                // V2 never creates its buyback account. Without an existing
                // one, list the legacy layout, whose v1 swap does.
                let buyback = keys
                    .v2_eligible()
                    .then(|| FeeRecipients::buyback(&global.data, quote.mint, quote.token_program))
                    .transpose()?;
                self.prefetch(
                    recipients
                        .addresses()
                        .into_iter()
                        .chain(buyback.iter().flat_map(FeeRecipients::addresses)),
                )?;
                recipients.prefer_initialized(&self.cached(&recipients.addresses()))?;
                keys.recipient = recipients.choose().0;
                if let Some(mut buyback) = buyback {
                    keys.v2 = buyback.prefer_initialized(&self.cached(&buyback.addresses()))?;
                    if keys.v2 {
                        keys.buyback = buyback.choose().0;
                    }
                }
                PoolAccounts::from_pump_keys(pool, wallet, x, base, keys)
            }
            abi::METEORA => {
                let a_vault = self.get(reads[0])?;
                let b_vault = self.get(reads[1])?;
                PoolAccounts::from_meteora_state(
                    pool,
                    &state.data,
                    &a_vault.data,
                    &b_vault.data,
                    x,
                    base,
                )
            }
            abi::CLMM | abi::PANCAKESWAP | abi::BYREAL => {
                let clmm = clmm::Pool::decode(pool, &state.data)?;
                let clock: Clock = self
                    .get(clock::id())?
                    .deserialize_data()
                    .context("invalid clock sysvar")?;
                clmm.ensure_swappable(state.owner, clock.unix_timestamp)?;
                let loaded = self.load_concentrated_pool(pool, &state, x, base)?;
                if state.owner == abi::BYREAL {
                    self.check_byreal_memo(pool, &loaded)?;
                }
                Ok(loaded)
            }
            abi::WHIRLPOOL | abi::DLMM => self.load_concentrated_pool(pool, &state, x, base),
            _ => PoolAccounts::from_state(pool, state.owner, &state.data, x, base, &[], None),
        }
    }

    /// A concentrated pool listing its bitmap extension and each direction's
    /// walk up to its first array that does not exist.
    fn load_concentrated_pool(
        &mut self,
        pool: Pubkey,
        state: &Account,
        x: MintAccounts,
        base: MintAccounts,
    ) -> Result<PoolAccounts> {
        let walks = self.walks(pool, state)?;
        self.prefetch(walks.concat())?;
        let arrays = arrays::listed_arrays(&walks, |array| self.exists(array, state.owner));
        ensure!(
            !arrays.is_empty(),
            "pool {pool} has no array where a swap starts"
        );
        // A CLMM fork lists its extension's address whether or not it exists;
        // DLMM's is optional on chain and listed only when it exists.
        let bitmap = abi::bitmap_extension(pool, state.owner)
            .filter(|key| state.owner != abi::DLMM || self.exists(key, state.owner));
        PoolAccounts::from_state(pool, state.owner, &state.data, x, base, &arrays, bitmap)
    }

    /// Rejects a Byreal pool, listing `loaded`, that lists the memo program
    /// while its bitmap extension holds no data: the executor rejects the
    /// whole transaction then. The extension must be prefetched.
    fn check_byreal_memo(&self, pool: Pubkey, loaded: &PoolAccounts) -> Result<()> {
        let lists_memo = loaded.accounts().iter().any(|a| a.pubkey == abi::MEMO);
        let extension_has_data = abi::bitmap_extension(pool, abi::BYREAL)
            .and_then(|key| self.fetched(&key))
            .is_some_and(|extension| !extension.data.is_empty());
        ensure!(
            !lists_memo || extension_has_data,
            "Byreal pool {pool} lists the memo program without a bitmap extension"
        );
        Ok(())
    }
}

/// Accounts a pool's builder reads besides the pool and its arrays: pump's
/// global config, Meteora DAMM v1's vault A and vault B states, or a
/// concentrated pool's bitmap extension; a CLMM pool also reads the clock
/// its opening time is checked against.
fn pool_reads(pool: Pubkey, state: &Account) -> Result<Vec<Pubkey>> {
    Ok(match state.owner {
        abi::PUMP => vec![pump_global_config()],
        abi::METEORA => abi::PoolPair::read(state.owner, &state.data)?
            .vaults()
            .to_vec(),
        abi::CLMM | abi::PANCAKESWAP | abi::BYREAL => abi::bitmap_extension(pool, state.owner)
            .into_iter()
            .chain([clock::id()])
            .collect(),
        owner => abi::bitmap_extension(pool, owner).into_iter().collect(),
    })
}

fn pump_global_config() -> Pubkey {
    Pubkey::find_program_address(&[b"global_config"], &abi::PUMP).0
}
