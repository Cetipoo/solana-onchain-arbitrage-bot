//! Batched, memoized account reads for building each mint's pools.
use crate::archer::{self, ARCHER};
use crate::dlmm_pro::{self, DLMM_PRO};
use crate::pump_fees::FeeRecipients;
use crate::v10::{self as abi, MintAccounts, PoolAccounts};
use anyhow::{bail, ensure, Context, Result};
use solana_client::{
    rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig, UiAccountEncoding},
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_sdk::{
    account::Account,
    pubkey::Pubkey,
    sysvar::{self, clock, last_restart_slot::ID as LAST_RESTART_SLOT},
};
use std::collections::{BTreeSet, HashMap};

/// Arrays loaded on each side of a concentrated pool's active one.
const ARRAY_RADIUS: u8 = 1;

/// Fetches accounts in batches and memoizes them for one load, so a mint's
/// pools cost a few round trips instead of one request per account.
pub struct AccountLoader<'a> {
    rpc: &'a RpcClient,
    accounts: HashMap<Pubkey, Option<Account>>,
    /// Each scanned Archer market's maker books.
    archer_books: HashMap<Pubkey, Vec<(Pubkey, Account)>>,
}

impl<'a> AccountLoader<'a> {
    pub fn new(rpc: &'a RpcClient) -> Self {
        Self {
            rpc,
            accounts: HashMap::new(),
            archer_books: HashMap::new(),
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

    /// Fetches the pools, every account their builders read, and then the
    /// arrays a DLMM Pro pool's bitmaps select. A pool that is missing or
    /// unreadable is left for `load_pool` to report.
    pub fn prefetch_pools(&mut self, pools: impl IntoIterator<Item = Pubkey>) -> Result<()> {
        let pools: Vec<Pubkey> = pools.into_iter().collect();
        self.prefetch(pools.iter().copied())?;
        let mut reads = Vec::new();
        for pool in &pools {
            if let Some(state) = self.accounts[pool].as_ref() {
                reads.extend(pool_reads(*pool, state).unwrap_or_default());
            }
        }
        self.prefetch(reads)?;
        let mut arrays = Vec::new();
        for pool in &pools {
            if let Some(state) = self.accounts[pool].as_ref() {
                if state.owner == DLMM_PRO {
                    let walks = self.dlmm_pro_walks(*pool, state);
                    arrays.extend(walks.map(|walks| walks.concat()).unwrap_or_default());
                }
            }
        }
        self.prefetch(arrays)
    }

    fn account(&mut self, key: Pubkey) -> Result<Option<&Account>> {
        self.prefetch([key])?;
        Ok(self.accounts[&key].as_ref())
    }

    /// An account this load already fetched, `None` where it does not exist.
    fn fetched(&self, key: &Pubkey) -> Result<Option<&Account>> {
        Ok(self
            .accounts
            .get(key)
            .with_context(|| format!("account {key} was not prefetched"))?
            .as_ref())
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
            abi::MANIFEST => {
                let market =
                    PoolAccounts::from_state(pool, abi::MANIFEST, &state.data, x, base, &[], None)?;
                let mut globals = Vec::new();
                for mint in abi::manifest_global_mints(&state.data)? {
                    let token_program = [x, base]
                        .into_iter()
                        .find(|declared| declared.mint == mint)
                        .with_context(|| format!("Manifest market {pool} does not trade {mint}"))?
                        .token_program;
                    if self.lists_manifest_global(&mint, &token_program)? {
                        globals.push(mint);
                    }
                }
                market.with_manifest_globals(&globals)
            }
            ARCHER => {
                archer::check_market(&pool, &state.data)?;
                let slot = clock_slot(&self.get(clock::ID)?)?;
                let last_restart_slot = last_restart_slot(&self.get(LAST_RESTART_SLOT)?)?;
                let books = archer::select_books(
                    &pool,
                    &wallet,
                    slot,
                    last_restart_slot,
                    self.scan_archer_books(pool)?
                        .iter()
                        .map(|(key, account)| archer::Book {
                            key: *key,
                            data: &account.data,
                        }),
                )?;
                PoolAccounts::from_archer_state(pool, &state.data, x, base, &books)
            }
            DLMM_PRO => self.load_dlmm_pro(pool, &state, x, base),
            abi::BYREAL => {
                self.check_byreal(pool, &state, x, base)?;
                self.load_with_arrays(pool, &state, x, base)
            }
            _ => self.load_with_arrays(pool, &state, x, base),
        }
    }

    /// A pool listing the prefetched arrays around its active one, and for
    /// DLMM its bitmap extension where that exists.
    fn load_with_arrays(
        &self,
        pool: Pubkey,
        state: &Account,
        x: MintAccounts,
        base: MintAccounts,
    ) -> Result<PoolAccounts> {
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

    /// A DLMM Pro pool listing the existing prefix of each direction's array
    /// walk. This client lists no transfer-hook accounts, so a pool whose
    /// Token-2022 mint names a hook program is rejected.
    fn load_dlmm_pro(
        &mut self,
        pool: Pubkey,
        state: &Account,
        x: MintAccounts,
        base: MintAccounts,
    ) -> Result<PoolAccounts> {
        for side in [x, base] {
            if side.token_program == abi::TOKEN_2022
                && dlmm_pro::has_transfer_hook(&self.get(side.mint)?.data)?
            {
                bail!(
                    "DLMM Pro pool {pool} mint {} has a transfer hook this client does not list",
                    side.mint
                );
            }
        }
        let walks = self.dlmm_pro_walks(pool, state)?;
        self.prefetch(walks.iter().flatten().copied())?;
        let walks = dlmm_pro::existing_prefix(walks, |array| {
            self.accounts[array]
                .as_ref()
                .is_some_and(|a| a.owner == DLMM_PRO)
        });
        let arrays = dlmm_pro::select_arrays(&walks);
        ensure!(
            !arrays.is_empty(),
            "pool {pool} has no array where a swap starts"
        );
        let bitmap = dlmm_pro_extension(pool, state);
        PoolAccounts::from_state(pool, DLMM_PRO, &state.data, x, base, &arrays, bitmap)
    }

    /// Each direction's array walk over the pool's own bitmap and, for a pool
    /// flagged as having one, the bitmap extension this load fetched.
    fn dlmm_pro_walks(&self, pool: Pubkey, state: &Account) -> Result<[Vec<Pubkey>; 2]> {
        let extension = dlmm_pro_extension(pool, state)
            .map(|key| self.fetched(&key))
            .transpose()?
            .flatten()
            .map(|account| account.data.as_slice());
        dlmm_pro::swap_walks(
            &pool,
            &state.data,
            extension,
            dlmm_pro::ARRAYS_PER_DIRECTION,
        )
    }

    /// The executor rejects a whole transaction whose Byreal pool lists an
    /// account other than a Pyth receiver update of the pool's feed, or lists
    /// the memo program without a bitmap extension holding data.
    fn check_byreal(
        &self,
        pool: Pubkey,
        state: &Account,
        x: MintAccounts,
        base: MintAccounts,
    ) -> Result<()> {
        let updates = abi::byreal_price_updates(&state.data)?;
        for update in updates.into_iter().flatten() {
            let key = update.account;
            let account = self.fetched(&key)?.with_context(|| {
                format!("Byreal pool {pool}: Pyth price update {key} not found")
            })?;
            update
                .check(account)
                .with_context(|| format!("Byreal pool {pool}"))?;
        }
        let extension_has_data = self
            .fetched(&abi::clmm_bitmap_extension(&pool, &abi::BYREAL))?
            .is_some_and(|account| !account.data.is_empty());
        ensure!(
            extension_has_data || !abi::lists_memo(abi::BYREAL, x, base, updates.is_some()),
            "Byreal pool {pool} lists the memo program without a bitmap extension"
        );
        Ok(())
    }

    /// Whether a Manifest market lists `mint`'s global: where the global
    /// exists and its vault is an initialized, unfrozen token account of
    /// `mint` under `token_program`. The executor rejects a listed frozen
    /// vault, and without the global a global order ends its walk.
    fn lists_manifest_global(&self, mint: &Pubkey, token_program: &Pubkey) -> Result<bool> {
        let [global, vault] = abi::manifest_global(mint);
        Ok(self
            .fetched(&global)?
            .is_some_and(|account| account.owner == abi::MANIFEST)
            && self.fetched(&vault)?.is_some_and(|account| {
                abi::is_initialized_token_account(account, mint, token_program)
            }))
    }

    /// The accounts the Archer program holds for `market`'s maker books, from
    /// a program scan at the client's commitment.
    fn scan_archer_books(&mut self, market: Pubkey) -> Result<&[(Pubkey, Account)]> {
        if !self.archer_books.contains_key(&market) {
            let config = RpcProgramAccountsConfig {
                filters: Some(vec![
                    RpcFilterType::DataSize(archer::BOOK_LEN as u64),
                    RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                        archer::BOOK_MARKET,
                        market.to_bytes().to_vec(),
                    )),
                ]),
                account_config: RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    ..RpcAccountInfoConfig::default()
                },
                ..RpcProgramAccountsConfig::default()
            };
            let mut books = Vec::new();
            for (key, account) in self
                .rpc
                .get_program_ui_accounts_with_config(&ARCHER, config)
                .with_context(|| format!("scan Archer market {market}'s maker books"))?
            {
                let account = account
                    .to_account()
                    .with_context(|| format!("undecodable Archer maker book {key}"))?;
                books.push((key, account));
            }
            self.archer_books.insert(market, books);
        }
        Ok(&self.archer_books[&market])
    }
}

/// Accounts a pool's builder or its loader checks read besides the pool. A
/// DLMM bitmap extension is optional on chain and only listed when it exists.
fn pool_reads(pool: Pubkey, state: &Account) -> Result<Vec<Pubkey>> {
    match state.owner {
        abi::PUMP => Ok(vec![pump_global_config()]),
        abi::METEORA => Ok(abi::PoolPair::read(state.owner, &state.data)?
            .vaults()
            .to_vec()),
        abi::MANIFEST => Ok(abi::manifest_global_mints(&state.data)?
            .iter()
            .flat_map(abi::manifest_global)
            .collect()),
        ARCHER => Ok(vec![clock::ID, LAST_RESTART_SLOT]),
        DLMM_PRO => Ok(dlmm_pro_extension(pool, state).into_iter().collect()),
        abi::BYREAL => {
            let (candidates, extension) =
                abi::array_candidates(pool, abi::BYREAL, &state.data, ARRAY_RADIUS)?;
            let updates = abi::byreal_price_updates(&state.data)?;
            Ok(candidates
                .into_iter()
                .chain(extension)
                .chain(updates.into_iter().flatten().map(|update| update.account))
                .collect())
        }
        owner => {
            let (candidates, bitmap) =
                abi::array_candidates(pool, owner, &state.data, ARRAY_RADIUS)?;
            let bitmap = bitmap.filter(|_| owner == abi::DLMM);
            Ok(candidates.into_iter().chain(bitmap).collect())
        }
    }
}

/// The bitmap extension of a DLMM Pro pool flagged as having one.
fn dlmm_pro_extension(pool: Pubkey, state: &Account) -> Option<Pubkey> {
    dlmm_pro::requires_bitmap_extension(&state.data).then(|| dlmm_pro::bitmap_extension(&pool))
}

/// The Clock's slot, the first of its five 8-byte fields.
fn clock_slot(account: &Account) -> Result<u64> {
    let clock = sysvar_data(account, "Clock", 40)?;
    Ok(u64::from_le_bytes(abi::read(clock, 0)?))
}

/// The Archer aggregator skips books updated at or before this slot.
fn last_restart_slot(account: &Account) -> Result<u64> {
    let slot = sysvar_data(account, "LastRestartSlot", 8)?;
    Ok(u64::from_le_bytes(abi::read(slot, 0)?))
}

/// The data of a sysvar account, which has a fixed size `len`.
fn sysvar_data<'a>(account: &'a Account, name: &str, len: usize) -> Result<&'a [u8]> {
    ensure!(
        account.owner == sysvar::ID && !account.executable,
        "invalid {name} sysvar account"
    );
    ensure!(account.data.len() == len, "invalid {name} sysvar size");
    Ok(&account.data)
}

fn pump_global_config() -> Pubkey {
    Pubkey::find_program_address(&[b"global_config"], &abi::PUMP).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v10::tests::price_update;

    fn account(owner: Pubkey, data: Vec<u8>) -> Account {
        Account {
            owner,
            data,
            ..Account::default()
        }
    }

    fn loader<'a>(rpc: &'a RpcClient, accounts: &[(Pubkey, Option<Account>)]) -> AccountLoader<'a> {
        let mut loader = AccountLoader::new(rpc);
        loader.accounts.extend(accounts.iter().cloned());
        loader
    }

    /// Each slot is the sysvar's first 8 bytes, read only from a sysvar
    /// account of the sysvar's size: the `Clock` struct's, or 8 bytes.
    #[test]
    fn a_sysvar_slot_is_read_only_from_a_sysvar_account_of_its_size() {
        let readers = [
            (
                "Clock",
                clock_slot as fn(&Account) -> Result<u64>,
                std::mem::size_of::<clock::Clock>(),
            ),
            ("LastRestartSlot", last_restart_slot, 8),
        ];
        for (name, read, len) in readers {
            let mut data = vec![0xff; len];
            data[..8].copy_from_slice(&471_000_123u64.to_le_bytes());
            assert_eq!(
                read(&account(sysvar::ID, data.clone())).unwrap(),
                471_000_123,
                "{name}"
            );
            let error = |account: Account| read(&account).unwrap_err().to_string();
            let size = format!("invalid {name} sysvar size");
            let mut long = data.clone();
            long.push(0);
            assert_eq!(error(account(sysvar::ID, long)), size);
            assert_eq!(error(account(sysvar::ID, data[..len - 1].to_vec())), size);
            let invalid = format!("invalid {name} sysvar account");
            assert_eq!(error(account(Pubkey::new_unique(), data.clone())), invalid);
            let mut executable = account(sysvar::ID, data);
            executable.executable = true;
            assert_eq!(error(executable), invalid);
        }
    }

    /// A token account of `mint` in `state`, the base layout's byte 108.
    fn token_account(program: Pubkey, mint: Pubkey, state: u8, len: usize) -> Account {
        let mut data = vec![0; len];
        data[..32].copy_from_slice(mint.as_ref());
        data[108] = state;
        account(program, data)
    }

    #[test]
    fn a_manifest_global_is_listed_only_with_an_open_vault_of_its_mint() {
        let rpc = RpcClient::new_mock("succeeds".to_string());
        let mint = Pubkey::new_unique();
        let [global, vault] = abi::manifest_global(&mint);
        let global_account = Some(account(abi::MANIFEST, vec![0; 8]));
        let lists = |global_account: Option<Account>, vault_account: Option<Account>| {
            loader(&rpc, &[(global, global_account), (vault, vault_account)])
                .lists_manifest_global(&mint, &abi::TOKEN)
                .unwrap()
        };
        let open = |program, mint, len| Some(token_account(program, mint, 1, len));
        assert!(lists(global_account.clone(), open(abi::TOKEN, mint, 165)));
        assert!(lists(global_account.clone(), open(abi::TOKEN, mint, 200)));
        let refused = [
            ("no global", None, open(abi::TOKEN, mint, 165)),
            (
                "global of another program",
                Some(account(Pubkey::new_unique(), vec![0; 8])),
                open(abi::TOKEN, mint, 165),
            ),
            ("no vault", global_account.clone(), None),
            (
                "frozen vault",
                global_account.clone(),
                Some(token_account(abi::TOKEN, mint, 2, 165)),
            ),
            (
                "uninitialized vault",
                global_account.clone(),
                Some(token_account(abi::TOKEN, mint, 0, 165)),
            ),
            (
                "vault of another mint",
                global_account.clone(),
                open(abi::TOKEN, Pubkey::new_unique(), 165),
            ),
            (
                "vault of another token program",
                global_account.clone(),
                open(abi::TOKEN_2022, mint, 165),
            ),
            (
                "truncated vault",
                global_account.clone(),
                Some(account(abi::TOKEN, {
                    let mut data = token_account(abi::TOKEN, mint, 1, 165).data;
                    data.pop();
                    data
                })),
            ),
        ];
        for (case, global_account, vault_account) in &refused {
            assert!(
                !lists(global_account.clone(), vault_account.clone()),
                "{case}"
            );
        }
        let unfetched = loader(&rpc, &[(vault, open(abi::TOKEN, mint, 165))]);
        assert_eq!(
            unfetched
                .lists_manifest_global(&mint, &abi::TOKEN)
                .unwrap_err()
                .to_string(),
            format!("account {global} was not prefetched")
        );
    }

    /// A mint of `owner` whose data is a Token-2022 mint (account type 1 at
    /// byte 165) with one transfer-hook extension (type 14, 64 bytes) naming
    /// `program` after its authority.
    fn hooked_mint(owner: Pubkey, program: Pubkey) -> Account {
        let mut data = vec![0; 166];
        data[165] = 1;
        data.extend(14u16.to_le_bytes());
        data.extend(64u16.to_le_bytes());
        data.extend(Pubkey::new_unique().to_bytes());
        data.extend(program.to_bytes());
        account(owner, data)
    }

    #[test]
    fn a_dlmm_pro_pool_of_a_hooked_token_2022_mint_is_refused() {
        let rpc = RpcClient::new_mock("succeeds".to_string());
        let wallet = Pubkey::new_unique();
        let pool = Pubkey::new_unique();
        let state = account(DLMM_PRO, vec![]);
        let load = |sides: [(Pubkey, Pubkey); 2]| {
            let [x, base] = sides.map(|(program, hook)| {
                let mint = MintAccounts::ata(&wallet, Pubkey::new_unique(), program).unwrap();
                (mint, hooked_mint(program, hook))
            });
            let error = loader(&rpc, &[(x.0.mint, Some(x.1)), (base.0.mint, Some(base.1))])
                .load_dlmm_pro(pool, &state, x.0, base.0)
                .unwrap_err()
                .to_string();
            (error, [x.0.mint, base.0.mint])
        };
        let hook = |mint: Pubkey| {
            format!(
                "DLMM Pro pool {pool} mint {mint} has a transfer hook this client does not list"
            )
        };
        // Past the hook check, the empty pool state fails its own check.
        let past_hooks = format!("truncated DLMM Pro pool {pool}");
        let (t22, legacy) = (abi::TOKEN_2022, abi::TOKEN);
        let (hooked, unhooked) = (Pubkey::new_unique(), Pubkey::default());

        let (error, [x, _]) = load([(t22, hooked), (t22, unhooked)]);
        assert_eq!(error, hook(x));
        let (error, [_, base]) = load([(t22, unhooked), (t22, hooked)]);
        assert_eq!(error, hook(base));
        // A mint declared SPL Token is not read for a hook.
        let (error, _) = load([(legacy, hooked), (legacy, hooked)]);
        assert_eq!(error, past_hooks);
        let (error, _) = load([(t22, unhooked), (t22, unhooked)]);
        assert_eq!(error, past_hooks);
    }

    /// A Byreal pool's state, flagged swap-dynamic-fee with `feeds` at the
    /// feed-id offsets 1112 and 1144 when `feeds` is given.
    fn byreal_state(feeds: Option<[[u8; 32]; 2]>) -> Account {
        let mut data = vec![0; 1176];
        if let Some(feeds) = feeds {
            data[1096] = 1 << 4;
            data[1112..1144].copy_from_slice(&feeds[0]);
            data[1144..1176].copy_from_slice(&feeds[1]);
        }
        account(abi::BYREAL, data)
    }

    #[test]
    fn a_byreal_pool_is_checked_against_its_price_updates_and_extension() {
        let rpc = RpcClient::new_mock("succeeds".to_string());
        let wallet = Pubkey::new_unique();
        let legacy = MintAccounts::ata(&wallet, Pubkey::new_unique(), abi::TOKEN).unwrap();
        let token_2022 = MintAccounts::ata(&wallet, Pubkey::new_unique(), abi::TOKEN_2022).unwrap();
        let sol = MintAccounts::ata(&wallet, abi::SOL, abi::TOKEN).unwrap();
        let pool = Pubkey::new_unique();
        let extension = Pubkey::find_program_address(
            &[b"pool_tick_array_bitmap_extension", pool.as_ref()],
            &abi::BYREAL,
        )
        .0;
        let feeds = [[1; 32], [2; 32]];
        let state = byreal_state(Some(feeds));
        let updates = abi::byreal_price_updates(&state.data)
            .unwrap()
            .unwrap()
            .map(|update| update.account);
        let update = |feed| Some(account(abi::PYTH_RECEIVER, price_update(1, feed, 133)));
        let with_data = Some(account(abi::BYREAL, vec![0; 8]));
        let check = |state: &Account, x, accounts: Vec<(Pubkey, Option<Account>)>| {
            loader(&rpc, &accounts)
                .check_byreal(pool, state, x, sol)
                .map_err(|error| format!("{error:#}"))
        };
        let dynamic = |extension_account: Option<Account>, first: Option<Account>| {
            vec![
                (extension, extension_account),
                (updates[0], first),
                (updates[1], update(feeds[1])),
            ]
        };
        assert_eq!(
            check(&state, legacy, dynamic(with_data.clone(), update(feeds[0]))),
            Ok(())
        );
        let memo = format!("Byreal pool {pool} lists the memo program without a bitmap extension");
        assert_eq!(
            check(&state, legacy, dynamic(None, update(feeds[0]))),
            Err(memo.clone())
        );
        let update_error = |first: Option<Account>| {
            check(&state, legacy, dynamic(with_data.clone(), first)).unwrap_err()
        };
        let first = updates[0];
        assert_eq!(
            update_error(None),
            format!("Byreal pool {pool}: Pyth price update {first} not found")
        );
        let foreign = Pubkey::new_unique();
        assert_eq!(
            update_error(Some(account(foreign, price_update(1, feeds[0], 133)))),
            format!("Byreal pool {pool}: Pyth price update {first} is owned by {foreign}")
        );
        assert_eq!(
            update_error(update(feeds[1])),
            format!("Byreal pool {pool}: Pyth price update {first} has another feed")
        );
        assert_eq!(
            update_error(Some(account(abi::PYTH_RECEIVER, price_update(1, feeds[0], 132)))),
            format!(
                "Byreal pool {pool}: invalid Pyth price update {first}: Pyth price update data too short"
            )
        );

        // A pool without the flag lists the memo program only for a
        // Token-2022 mint, and then needs an extension holding data.
        let plain = byreal_state(None);
        let empty = Some(account(abi::BYREAL, vec![]));
        assert_eq!(check(&plain, legacy, vec![(extension, None)]), Ok(()));
        assert_eq!(
            check(&plain, token_2022, vec![(extension, with_data)]),
            Ok(())
        );
        for extension_account in [None, empty] {
            assert_eq!(
                check(&plain, token_2022, vec![(extension, extension_account)]),
                Err(memo.clone())
            );
        }
        assert_eq!(
            check(&plain, legacy, vec![]),
            Err(format!("account {extension} was not prefetched"))
        );
    }
}
