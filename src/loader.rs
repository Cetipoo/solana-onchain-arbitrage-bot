//! Batched, memoized account reads for building each mint's pools.
use crate::v10::{self as abi, MintAccounts, PoolAccounts};
use anyhow::{ensure, Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{account::Account, pubkey::Pubkey};
use std::collections::{BTreeSet, HashMap};

/// Arrays loaded on each side of a concentrated pool's active one.
const ARRAY_RADIUS: u8 = 1;

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
    /// their builders read. A pool that is missing or unreadable is left for
    /// `load_pool` to report.
    pub fn prefetch_pools(&mut self, pools: impl IntoIterator<Item = Pubkey>) -> Result<()> {
        let pools: Vec<Pubkey> = pools.into_iter().collect();
        self.prefetch(pools.iter().copied())?;
        let mut reads = Vec::new();
        for pool in pools {
            if let Some(state) = self.accounts[&pool].as_ref() {
                reads.extend(pool_reads(pool, state).unwrap_or_default());
            }
        }
        self.prefetch(reads)
    }

    fn account(&mut self, key: Pubkey) -> Result<Option<&Account>> {
        self.prefetch([key])?;
        Ok(self.accounts[&key].as_ref())
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
                let mut recipients = crate::pump_fees::FeeRecipients::new(
                    &global.data,
                    state.data.get(243) == Some(&1),
                    keys.quote.mint,
                    keys.quote.token_program,
                )?;
                let addresses = recipients.addresses();
                self.prefetch(addresses.iter().copied())?;
                recipients.prefer_initialized(
                    &addresses
                        .iter()
                        .map(|key| self.accounts[key].clone())
                        .collect::<Vec<_>>(),
                )?;
                keys.recipient = recipients.choose().0;
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
            _ => {
                let (candidates, bitmap) =
                    abi::array_candidates(pool, state.owner, &state.data, ARRAY_RADIUS)?;
                let exists = |key: &Pubkey| {
                    self.accounts[key]
                        .as_ref()
                        .is_some_and(|a| a.owner == state.owner)
                };
                let arrays: Vec<Pubkey> = candidates.into_iter().filter(exists).collect();
                let bitmap = bitmap.filter(|key| state.owner != abi::DLMM || exists(key));
                PoolAccounts::from_state(pool, state.owner, &state.data, x, base, &arrays, bitmap)
            }
        }
    }
}

/// Accounts a pool's builder reads besides the pool: pump's global config,
/// Meteora DAMM v1's vault A and vault B states, or the arrays around the
/// active tick plus, for DLMM, the bitmap extension, which is optional on
/// chain and only listed when it exists.
fn pool_reads(pool: Pubkey, state: &Account) -> Result<Vec<Pubkey>> {
    match state.owner {
        abi::PUMP => Ok(vec![pump_global_config()]),
        abi::METEORA => Ok(abi::PoolPair::read(state.owner, &state.data)?
            .vaults()
            .to_vec()),
        _ => {
            let (candidates, bitmap) =
                abi::array_candidates(pool, state.owner, &state.data, ARRAY_RADIUS)?;
            let bitmap = bitmap.filter(|_| state.owner == abi::DLMM);
            Ok(candidates.into_iter().chain(bitmap).collect())
        }
    }
}

fn pump_global_config() -> Pubkey {
    Pubkey::find_program_address(&[b"global_config"], &abi::PUMP).0
}
