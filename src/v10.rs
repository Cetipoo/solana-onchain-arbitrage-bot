//! V10 instruction builder for the on-chain arbitrage program.
use crate::{
    archer::{self, ARCHER},
    dlmm_pro::{self, DLMM_PRO, DLMM_PRO_EVENT_AUTHORITY, DLMM_PRO_POOL_AUTHORITY},
    hylo::{HyloPool, HYLO_EXCHANGE, HYLO_STABILITY},
    stake_pool::{StakePool, SPL_STAKE_POOL},
};
use anyhow::{bail, ensure, Context, Result};
use executor_v10_abi::{
    Group, InstructionData, Venue, ARCHER_MAX_BOOKS, MANIFEST_POOL_ACCOUNTS, MAX_GROUPS,
    MAX_PAYLOAD_LEN, MAX_POOLS, PUMP_V2_POOL_ACCOUNTS,
};
pub use executor_v10_abi::{Header, OPCODE};
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    pubkey,
    pubkey::Pubkey,
};

pub const CPMM: Pubkey = pubkey!("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C");
pub const CLMM: Pubkey = pubkey!("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");
pub const PANCAKESWAP: Pubkey = pubkey!("HpNfyc2Saw7RKkQd8nEL4khUcuPhQ7WwY1B2qjx8jxFq");
pub const BYREAL: Pubkey = pubkey!("REALQqNEomY6cQGZJUGwywTBD2UmDT32rZcNnfxQ5N2");
pub const WHIRLPOOL: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
pub const DLMM: Pubkey = pubkey!("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");
pub const PUMP: Pubkey = pubkey!("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
pub const PUMP_FEE: Pubkey = pubkey!("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");
pub const CLMM_CONVERSION: Pubkey = pubkey!("3ucNos4NbumPLZNWztqGHNFFgkHeRMBQAVemeeomsUxv");
pub const RAYDIUM: Pubkey = pubkey!("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");
pub const RAYDIUM_CONVERSION: Pubkey = pubkey!("58oQChx4yWmvKdwLLZzBi4ChoCc2fqCUWBkwMihLYQo2");
pub const ORCA_CONVERSION: Pubkey = pubkey!("Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE");

pub const MANIFEST: Pubkey = pubkey!("MNFSTqtC93rEfYHB6hF82sKdZpUDFWkViLByLd1k1Ms");
pub const DAMMV2: Pubkey = pubkey!("cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG");
/// Meteora DAMM v1 (dynamic pools) and the vault program its swaps deposit into.
pub const METEORA: Pubkey = pubkey!("Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB");
pub const METEORA_VAULT: Pubkey = pubkey!("24Uqj9JCLxUeoC3hGfh5W3s9FM9uCHDS2SG3LYwBpyTi");
/// MetaDAO's Futarchy program, whose Dao accounts hold a spot pool.
pub const FUTARCHY: Pubkey = pubkey!("FUTARELBfJfQ8RDGhg1wdhddq1odMAJUePHFuBYfUxKq");
/// Pyth's push oracle, which keeps the sponsored price feed accounts.
pub const PYTH_PUSH_ORACLE: Pubkey = pubkey!("pythWSnswVUd12oZpeFP8e9CVaEqJg25g1Vtc2biRsT");
/// The Pyth receiver program, which owns the price updates a Byreal
/// swap-dynamic-fee pool reads.
pub const PYTH_RECEIVER: Pubkey = pubkey!("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");
const PRICE_UPDATE_DISCRIMINATOR: [u8; 8] = [0x22, 0xf1, 0x23, 0x63, 0x9d, 0x7e, 0xf4, 0xcd];

pub const SOL: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
pub const USDC: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
pub const TOKEN: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const TOKEN_2022: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
pub const MEMO: Pubkey = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
pub const ASSOCIATED: Pubkey = pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
pub const SYSTEM: Pubkey = pubkey!("11111111111111111111111111111111");

#[cfg(test)]
#[path = "v10_tests.rs"]
pub(crate) mod tests;

/// The most tick or bin arrays a concentrated pool's block lists.
pub const MAX_ARRAYS: usize = 6;
pub(crate) const WHIRLPOOL_TICK_SPACING_OFFSET: usize = 41;
const WHIRLPOOL_FEE_TIER_INDEX_OFFSET: usize = 43;
const FUTARCHY_DAO_DISCRIMINATOR: [u8; 8] = [163, 9, 47, 31, 52, 85, 197, 49];
/// A DAMM v2 pool's `collect_fee_mode`, and the mode that compounds its fees
/// into its reserves.
const DAMMV2_COLLECT_FEE_MODE_OFFSET: usize = 484;
const DAMMV2_COMPOUNDING_FEE_MODE: u8 = 2;

pub(crate) fn read<const N: usize>(d: &[u8], offset: usize) -> Result<[u8; N]> {
    d.get(offset..offset + N)
        .ok_or_else(|| anyhow::anyhow!("truncated pool account"))?
        .try_into()
        .map_err(Into::into)
}

/// The tick array of a CLMM-fork `pool` under `program` whose first tick is
/// `start_tick`.
pub fn clmm_tick_array(pool: Pubkey, program: Pubkey, start_tick: i32) -> Pubkey {
    Pubkey::find_program_address(
        &[b"tick_array", pool.as_ref(), &start_tick.to_be_bytes()],
        &program,
    )
    .0
}

/// The tick array of Whirlpool `pool` whose first tick is `start_tick`.
pub fn whirlpool_tick_array(pool: Pubkey, start_tick: i32) -> Pubkey {
    Pubkey::find_program_address(
        &[
            b"tick_array",
            pool.as_ref(),
            start_tick.to_string().as_bytes(),
        ],
        &WHIRLPOOL,
    )
    .0
}

/// The bin array of DLMM `pool` at array `index`.
pub fn dlmm_bin_array(pool: Pubkey, index: i64) -> Pubkey {
    Pubkey::find_program_address(&[b"bin_array", pool.as_ref(), &index.to_le_bytes()], &DLMM).0
}

/// Whether pools of `owner` list tick or bin arrays.
fn uses_arrays(owner: Pubkey) -> bool {
    matches!(
        owner,
        CLMM | PANCAKESWAP | BYREAL | WHIRLPOOL | DLMM | DLMM_PRO
    )
}

/// The tick-array bitmap extension of a CLMM, PancakeSwap or Byreal pool.
pub fn clmm_bitmap_extension(pool: &Pubkey, program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"pool_tick_array_bitmap_extension", pool.as_ref()],
        program,
    )
    .0
}

/// The bin-array bitmap extension of a DLMM pool.
pub fn dlmm_bitmap_extension(pool: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"bitmap", pool.as_ref()], &DLMM).0
}

#[derive(Clone, Copy, Debug)]
pub struct MintAccounts {
    pub mint: Pubkey,
    pub token_program: Pubkey,
    pub wallet: Pubkey,
}
impl MintAccounts {
    pub fn ata(wallet: &Pubkey, mint: Pubkey, token_program: Pubkey) -> Result<Self> {
        ensure!(
            [TOKEN, TOKEN_2022].contains(&token_program),
            "unsupported token program"
        );
        Ok(Self {
            mint,
            token_program,
            wallet: Pubkey::find_program_address(
                &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
                &ASSOCIATED,
            )
            .0,
        })
    }
    fn append(self, to: &mut Vec<AccountMeta>) {
        to.extend([
            AccountMeta::new_readonly(self.mint, false),
            AccountMeta::new_readonly(self.token_program, false),
            AccountMeta::new(self.wallet, false),
        ]);
    }
}

/// An SPL token account's base length, which Token-2022 extensions follow,
/// where it records its state, and the state of an account neither
/// uninitialized nor frozen.
const TOKEN_ACCOUNT_LEN: usize = 165;
const TOKEN_ACCOUNT_STATE_OFFSET: usize = 108;
const TOKEN_ACCOUNT_INITIALIZED: u8 = 1;

/// Whether `account` is an initialized, unfrozen token account of `mint`
/// under `token_program`.
pub(crate) fn is_initialized_token_account(
    account: &Account,
    mint: &Pubkey,
    token_program: &Pubkey,
) -> bool {
    account.owner == *token_program
        && account.data.len() >= TOKEN_ACCOUNT_LEN
        && account.data[..32] == mint.to_bytes()
        && account.data[TOKEN_ACCOUNT_STATE_OFFSET] == TOKEN_ACCOUNT_INITIALIZED
}

/// A mint's Manifest global account, whose deposits back the global orders
/// of every market of that mint, and the global's vault.
pub fn manifest_global(mint: &Pubkey) -> [Pubkey; 2] {
    [b"global".as_slice(), b"global-vault"]
        .map(|seed| Pubkey::find_program_address(&[seed, mint.as_ref()], &MANIFEST).0)
}

/// How many of each side's best orders a Manifest market is scanned for
/// global ones: more than the executor's order-book allowance funds a walk
/// through.
const MANIFEST_GLOBAL_SCAN: usize = 32;
const MANIFEST_NIL: u32 = u32::MAX;
const MANIFEST_HEADER_LEN: usize = 256;
/// A node's size; a node's index is its byte offset after the header.
const MANIFEST_NODE_LEN: usize = 80;
/// Where a Manifest market records the first node of its free list.
const MANIFEST_FREE_LIST: usize = 176;
/// Where a Manifest market records its best bid's and best ask's node.
const MANIFEST_BEST_BID: usize = 160;
const MANIFEST_BEST_ASK: usize = 168;
/// A resting order's type byte in its node, and the type of a global order.
const MANIFEST_ORDER_TYPE: usize = 57;
const MANIFEST_GLOBAL_ORDER: u8 = 3;

/// A Manifest market's order, seat and free nodes after its header, read
/// with bounds, alignment and cycle checks.
struct ManifestBook<'a>(&'a [u8]);

impl<'a> ManifestBook<'a> {
    fn node(&self, index: u32) -> Result<&'a [u8]> {
        ensure!(
            index.is_multiple_of(MANIFEST_NODE_LEN as u32),
            "misaligned Manifest node"
        );
        let start = MANIFEST_HEADER_LEN + index as usize;
        self.0
            .get(start..start + MANIFEST_NODE_LEN)
            .ok_or_else(|| anyhow::anyhow!("Manifest node out of range"))
    }

    fn link(&self, index: u32, at: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(read(self.node(index)?, at)?))
    }

    /// The next worse order: both sides store their best order at the
    /// largest tree key. A tree's path holds no more nodes than the market,
    /// so a longer one cycles.
    fn predecessor(&self, mut index: u32) -> Result<u32> {
        let nodes = self.0.len().saturating_sub(MANIFEST_HEADER_LEN) / MANIFEST_NODE_LEN;
        let left = self.link(index, 0)?;
        if left != MANIFEST_NIL {
            index = left;
            for _ in 0..nodes {
                let right = self.link(index, 4)?;
                if right == MANIFEST_NIL {
                    return Ok(index);
                }
                index = right;
            }
        } else {
            for _ in 0..nodes {
                let parent = self.link(index, 8)?;
                if parent == MANIFEST_NIL || self.link(parent, 0)? != index {
                    return Ok(parent);
                }
                index = parent;
            }
        }
        bail!("cyclic Manifest order tree")
    }

    /// Visits the best `MANIFEST_GLOBAL_SCAN` orders of the side whose best
    /// order `best` records, best first, while `visit` returns true.
    fn scan(&self, best: usize, mut visit: impl FnMut(&'a [u8]) -> Result<bool>) -> Result<()> {
        let mut index = u32::from_le_bytes(read(self.0, best)?);
        for _ in 0..MANIFEST_GLOBAL_SCAN {
            if index == MANIFEST_NIL || !visit(self.node(index)?)? {
                break;
            }
            index = self.predecessor(index)?;
        }
        Ok(())
    }

    /// The executor rejects a Manifest market unless its free list holds a
    /// node past the one a resting order would take, so a swap never grows
    /// the market.
    fn has_spare_node(&self) -> bool {
        let Ok(free) = read(self.0, MANIFEST_FREE_LIST).map(u32::from_le_bytes) else {
            return false;
        };
        self.link(free, 0)
            .is_ok_and(|next| next != free && self.node(next).is_ok())
    }
}

/// The mints whose globals back a global order among the best
/// `MANIFEST_GLOBAL_SCAN` orders of either side of a Manifest market: quote
/// deposits back bids, base deposits back asks. A global listed for a side
/// lets the executor fill those orders; without it a global order ends the
/// walk.
pub fn manifest_global_mints(data: &[u8]) -> Result<Vec<Pubkey>> {
    let pair = PoolPair::read(MANIFEST, data)?;
    let book = ManifestBook(data);
    let mut mints = Vec::new();
    for (best, mint) in [
        (MANIFEST_BEST_BID, pair.mints[1]),
        (MANIFEST_BEST_ASK, pair.mints[0]),
    ] {
        book.scan(best, |order| {
            let global = order[MANIFEST_ORDER_TYPE] == MANIFEST_GLOBAL_ORDER;
            if global {
                mints.push(mint);
            }
            Ok(!global)
        })?;
    }
    Ok(mints)
}

/// Whether a CLMM, DLMM or Whirlpool pool of `owner` trading `x` against
/// `base` lists the memo program: Whirlpool always, as V2 supports both token
/// programs and writable oracle accounts, a Byreal swap-dynamic-fee pool
/// always, as `swap_v3_dyn` takes it, and the others for a Token-2022 mint.
pub fn lists_memo(
    owner: Pubkey,
    x: MintAccounts,
    base: MintAccounts,
    swap_dynamic_fee: bool,
) -> bool {
    owner == WHIRLPOOL || swap_dynamic_fee || uses_token_2022(x, base)
}

fn uses_token_2022(x: MintAccounts, base: MintAccounts) -> bool {
    x.token_program == TOKEN_2022 || base.token_program == TOKEN_2022
}

/// A Byreal pool's flags byte, and the flag of a swap-dynamic-fee pool.
const BYREAL_FEE_FLAGS_OFFSET: usize = 1096;
const BYREAL_SWAP_DYNAMIC_FEE: u8 = 1 << 4;
/// Where a Byreal pool stores token0's and token1's Pyth feed ids.
const BYREAL_FEED_ID_OFFSETS: [usize; 2] = [1112, 1144];

/// A Pyth push-oracle price update account and the feed id it must carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PythPriceUpdate {
    pub account: Pubkey,
    pub feed: [u8; 32],
}

impl PythPriceUpdate {
    /// Fails unless `account`, the account at `self.account`, is a Pyth
    /// receiver update of `self.feed`.
    pub fn check(&self, account: &Account) -> Result<()> {
        let key = self.account;
        ensure!(
            account.owner == PYTH_RECEIVER,
            "Pyth price update {key} is owned by {}",
            account.owner
        );
        let feed = price_update_feed(&account.data)
            .with_context(|| format!("invalid Pyth price update {key}"))?;
        ensure!(
            feed == self.feed,
            "Pyth price update {key} has another feed"
        );
        Ok(())
    }
}

/// The feed id of a Pyth receiver `PriceUpdateV2`: after its discriminator
/// and write authority, a verification level of one byte for `Full` or two
/// for `Partial`, then the price message, 84 bytes, and the posted slot.
fn price_update_feed(data: &[u8]) -> Result<[u8; 32]> {
    ensure!(
        data.get(..8) == Some(&PRICE_UPDATE_DISCRIMINATOR[..]),
        "invalid Pyth price update discriminator"
    );
    let message = match data.get(40) {
        Some(0) => 42,
        Some(1) => 41,
        _ => bail!("unknown Pyth verification level"),
    };
    ensure!(
        data.len() >= message + 84 + 8,
        "Pyth price update data too short"
    );
    read(data, message)
}

/// The Pyth price updates a Byreal swap-dynamic-fee pool's `swap_v3_dyn`
/// reads, token0's then token1's: the push oracle's shard-0 accounts for the
/// feed ids the pool stores. `None` for a Byreal pool without the flag.
pub fn byreal_price_updates(data: &[u8]) -> Result<Option<[PythPriceUpdate; 2]>> {
    if read::<1>(data, BYREAL_FEE_FLAGS_OFFSET)?[0] & BYREAL_SWAP_DYNAMIC_FEE == 0 {
        return Ok(None);
    }
    let update = |offset: usize| -> Result<PythPriceUpdate> {
        let feed = read::<32>(data, offset)?;
        let account =
            Pubkey::find_program_address(&[&0u16.to_le_bytes(), &feed], &PYTH_PUSH_ORACLE).0;
        Ok(PythPriceUpdate { account, feed })
    };
    Ok(Some([
        update(BYREAL_FEED_ID_OFFSETS[0])?,
        update(BYREAL_FEED_ID_OFFSETS[1])?,
    ]))
}

/// Whether a Whirlpool's state names an adaptive-fee tier, the executor's
/// test: its fee-tier index differs from its tick spacing. The executor walks
/// fee groups only while the pool's adaptive-fee schedule is active, so a
/// pool this reports as fixed-fee is never walked with an adaptive fee.
fn whirlpool_state_adaptive_fee(data: &[u8]) -> Result<bool> {
    Ok(read::<2>(data, WHIRLPOOL_TICK_SPACING_OFFSET)?
        != read::<2>(data, WHIRLPOOL_FEE_TIER_INDEX_OFFSET)?)
}

fn is_hylo(program: &Pubkey) -> bool {
    [HYLO_EXCHANGE, HYLO_STABILITY].contains(program)
}

/// Canonical mint/vault layout shared by pool discovery and instruction
/// construction. For Meteora DAMM v1 the vaults are the vault-program
/// accounts, not token accounts; for Hylo and stake pools the issued side's
/// vault is its mint, whose supply is outstanding, and a stake pool's SOL
/// side is its reserve stake account.
pub struct PoolPair {
    pub mints: [Pubkey; 2],
    vaults: [Pubkey; 2],
}

impl PoolPair {
    /// An issuer's pair, whose issued side's vault is its mint.
    pub(crate) fn issued(mints: [Pubkey; 2], vaults: [Pubkey; 2]) -> Self {
        Self { mints, vaults }
    }

    /// The pair an account of `program` holding `data` pools; an error for a
    /// Hylo account that pools several conversions.
    pub fn read(program: Pubkey, data: &[u8]) -> Result<Self> {
        if is_hylo(&program) {
            let mut conversions = HyloPool::conversions(&program, data);
            return match (conversions.next(), conversions.next()) {
                (Some(hylo), None) => Ok(hylo.pair()),
                (None, _) => bail!("not the pool of a Hylo conversion the executor trades"),
                (Some(_), Some(_)) => {
                    bail!("not the pool of exactly one conversion the executor trades")
                }
            };
        }
        if program == SPL_STAKE_POOL {
            return Ok(StakePool::read(data)?.pair());
        }
        Self::layout(program, data)
    }

    /// The pair an account of `program` holding `data` pools between `a` and
    /// `b`, either way round. A Hylo account that pools several conversions
    /// is read as the one trading them.
    pub(crate) fn read_trading(program: Pubkey, data: &[u8], [a, b]: [Pubkey; 2]) -> Result<Self> {
        if is_hylo(&program) {
            return Ok(HyloPool::read(&program, data, [a, b])?.pair());
        }
        let pair = Self::read(program, data)?;
        pair.vaults_for(a, b)?;
        Ok(pair)
    }

    fn layout(program: Pubkey, data: &[u8]) -> Result<Self> {
        let (m0, m1, v0, v1) = match program {
            RAYDIUM => (400, 432, 336, 368),
            METEORA => (40, 72, 104, 136),
            CPMM => (168, 200, 72, 104),
            CLMM | PANCAKESWAP | BYREAL => (73, 105, 137, 169),
            WHIRLPOOL => (101, 181, 133, 213),
            DLMM => (88, 120, 152, 184),
            MANIFEST => {
                ensure!(
                    data.len() >= MANIFEST_HEADER_LEN
                        && data[8] == 0
                        && u64::from_le_bytes(read(data, 0)?) == 4859840929024028656,
                    "invalid Manifest market"
                );
                (16, 48, 80, 112)
            }
            DAMMV2 => (168, 200, 232, 264),
            PUMP => (43, 75, 139, 171),
            DLMM_PRO => (8, 40, 72, 104),
            ARCHER => {
                ensure!(archer::is_market(data), "invalid Archer market");
                archer::MARKET_PAIR
            }
            FUTARCHY => {
                // A live proposal splits the Dao's liquidity into spot, pass
                // and fail pools, which the executor does not trade.
                ensure!(
                    data.get(..8) == Some(&FUTARCHY_DAO_DISCRIMINATOR[..])
                        && data.get(8) == Some(&0),
                    "not a Futarchy Dao with a single spot pool"
                );
                (157, 189, 221, 253)
            }
            _ => bail!("unsupported V10 DEX {program}"),
        };
        let key = |offset| -> Result<Pubkey> { Ok(Pubkey::new_from_array(read(data, offset)?)) };
        Ok(Self {
            mints: [key(m0)?, key(m1)?],
            vaults: [key(v0)?, key(v1)?],
        })
    }

    pub fn vaults(&self) -> [Pubkey; 2] {
        self.vaults
    }

    fn vaults_for(&self, x: Pubkey, base: Pubkey) -> Result<[Pubkey; 2]> {
        ensure!(x != base, "pool mints must differ");
        if self.mints == [x, base] {
            Ok(self.vaults)
        } else if self.mints == [base, x] {
            Ok([self.vaults[1], self.vaults[0]])
        } else {
            bail!("pool does not match the declared mint pair")
        }
    }
}

/// DEX-specific accounts for a decoded pool. Vaults are supplied in X/base order.
#[derive(Clone, Copy, Debug)]
pub enum PoolKeys<'a> {
    Raydium,
    Cpmm {
        config: Pubkey,
        observation: Pubkey,
    },
    DammV2 {
        /// Whether the pool collects fees by compounding them into its
        /// liquidity, which the executor quotes as the costlier venue.
        compounding: bool,
    },
    Manifest,
    Futarchy,
    Clmm {
        program: Pubkey,
        config: Pubkey,
        observation: Pubkey,
        bitmap: Pubkey,
        arrays: &'a [Pubkey],
        /// A Byreal swap-dynamic-fee pool's token0 and token1 Pyth price
        /// updates, which also list the memo program.
        pyth: Option<[Pubkey; 2]>,
    },
    Whirlpool {
        arrays: &'a [Pubkey],
        /// `whirlpool_state_adaptive_fee` of the pool's state; `None` when
        /// unknown, which the cost model reserves as adaptive.
        adaptive_fee: Option<bool>,
    },
    Dlmm {
        oracle: Pubkey,
        bitmap: Option<Pubkey>,
        arrays: &'a [Pubkey],
    },
    DlmmPro {
        bitmap: Option<Pubkey>,
        /// The arrays both directions walk: `arrays::listed_arrays` of
        /// `dlmm_pro::swap_walks`.
        arrays: &'a [Pubkey],
    },
    Archer {
        /// The maker books a swap fills from, in the order the aggregator
        /// splits tied prices: `archer::select_books`.
        books: &'a [Pubkey],
    },
}
impl PoolKeys<'_> {
    fn program(self) -> Pubkey {
        match self {
            Self::Raydium => RAYDIUM,
            Self::Cpmm { .. } => CPMM,
            Self::DammV2 { .. } => DAMMV2,
            Self::Manifest => MANIFEST,
            Self::Futarchy => FUTARCHY,
            Self::Clmm { program, .. } => program,
            Self::Whirlpool { .. } => WHIRLPOOL,
            Self::Dlmm { .. } => DLMM,
            Self::DlmmPro { .. } => DLMM_PRO,
            Self::Archer { .. } => ARCHER,
        }
    }

    /// The venue the executor quotes the pool as.
    fn venue(self) -> Venue {
        match self {
            Self::Raydium => Venue::RaydiumAmm,
            Self::Cpmm { .. } => Venue::RaydiumCpmm,
            Self::DammV2 { compounding: true } => Venue::Dammv2Compounding,
            Self::DammV2 { compounding: false } => Venue::Dammv2,
            Self::Manifest => Venue::Manifest,
            Self::Futarchy => Venue::Futarchy,
            Self::Clmm { pyth: Some(_), .. } => Venue::ByrealDynamicFee,
            Self::Clmm { program, .. } if program == CLMM => Venue::Clmm,
            Self::Clmm { .. } => Venue::ClmmFork,
            Self::Whirlpool { .. } => Venue::Whirlpool,
            Self::Dlmm { .. } => Venue::Dlmm,
            Self::DlmmPro { .. } => Venue::DlmmPro,
            Self::Archer { .. } => Venue::Archer,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PumpKeys {
    pub vaults: [Pubkey; 2],
    pub mint0: Pubkey,
    pub quote: MintAccounts,
    pub recipient: Pubkey,
    pub creator: Pubkey,
    pub cashback: bool,
    pub buyback: Pubkey,
    /// List the compact layout the executor trades with PumpSwap v2. Only a
    /// caller that has seen `buyback`'s quote account exist may set it: v2
    /// never creates that account.
    pub v2: bool,
}

impl PumpKeys {
    pub fn from_state(
        data: &[u8],
        global: &[u8],
        x: MintAccounts,
        base: MintAccounts,
    ) -> Result<Self> {
        let key = |offset| -> Result<Pubkey> { Ok(Pubkey::new_from_array(read(data, offset)?)) };
        let pair = PoolPair::read(PUMP, data)?;
        let [mint0, mint1] = pair.mints;
        let [x_vault, base_vault] = pair.vaults_for(x.mint, base.mint)?;
        let quote = if mint1 == base.mint { base } else { x };
        let recipient_offset = if *data
            .get(243)
            .ok_or_else(|| anyhow::anyhow!("truncated Pump pool"))?
            == 0
        {
            57
        } else {
            385
        };
        let recipient = Pubkey::new_from_array(read(global, recipient_offset)?);
        Ok(Self {
            vaults: [x_vault, base_vault],
            mint0,
            quote,
            recipient,
            creator: key(211)?,
            cashback: data.get(244) == Some(&1),
            buyback: Pubkey::new_from_array(read(global, 643)?),
            v2: false,
        })
    }

    /// Whether PumpSwap v2 can trade the pool, given an existing buyback
    /// quote account.
    pub fn v2_eligible(&self) -> bool {
        !self.cashback
            && [SOL, USDC].contains(&self.quote.mint)
            && self.quote.token_program == TOKEN
    }
}

#[derive(Clone, Debug)]
pub struct PoolAccounts {
    venue: Venue,
    pool: Pubkey,
    x_mint: Pubkey,
    base_mint: Pubkey,
    accounts: Vec<AccountMeta>,
    /// Pump's orientation; `None` for every other venue.
    pump_quote_is_base: Option<bool>,
    /// A Whirlpool's `PoolKeys::Whirlpool::adaptive_fee`; `None` for every
    /// other venue.
    whirlpool_adaptive_fee: Option<bool>,
}
impl PoolAccounts {
    pub fn from_pump_keys(
        pool: Pubkey,
        wallet: Pubkey,
        x: MintAccounts,
        base: MintAccounts,
        keys: PumpKeys,
    ) -> Result<Self> {
        ensure!(
            [x.mint, base.mint].contains(&keys.quote.mint)
                && [x.mint, base.mint].contains(&keys.mint0)
                && keys.quote.mint != keys.mint0,
            "invalid Pump orientation"
        );
        ensure!(
            !keys.v2 || keys.v2_eligible(),
            "Pump v2 requires a non-cashback SOL or USDC pool"
        );
        let PumpKeys {
            vaults: [x_vault, base_vault],
            mint0,
            quote,
            recipient,
            creator,
            cashback,
            buyback,
            v2,
        } = keys;
        let pda = |seeds: &[&[u8]]| Pubkey::find_program_address(seeds, &PUMP).0;
        let ata = |owner: Pubkey| {
            Pubkey::find_program_address(
                &[
                    owner.as_ref(),
                    quote.token_program.as_ref(),
                    quote.mint.as_ref(),
                ],
                &ASSOCIATED,
            )
            .0
        };
        let volume = pda(&[b"user_volume_accumulator", wallet.as_ref()]);
        let fee_config = Pubkey::find_program_address(&[b"fee_config", PUMP.as_ref()], &PUMP_FEE).0;
        let mut accounts = vec![
            AccountMeta::new_readonly(PUMP, false),
            AccountMeta::new_readonly(base.mint, false),
            AccountMeta::new_readonly(pda(&[b"global_config"]), false),
            AccountMeta::new_readonly(pda(&[b"__event_authority"]), false),
        ];
        let venue = if v2 {
            accounts.extend([
                AccountMeta::new(pool, false),
                AccountMeta::new(x_vault, false),
                AccountMeta::new(base_vault, false),
                AccountMeta::new(volume, false),
                AccountMeta::new_readonly(fee_config, false),
                AccountMeta::new(ata(buyback), false),
            ]);
            debug_assert_eq!(accounts.len(), PUMP_V2_POOL_ACCOUNTS);
            Venue::PumpV2
        } else {
            let creator_vault = pda(&[b"creator_vault", creator.as_ref()]);
            accounts.extend([
                AccountMeta::new_readonly(recipient, false),
                AccountMeta::new(pool, false),
                AccountMeta::new(x_vault, false),
                AccountMeta::new(base_vault, false),
                AccountMeta::new(ata(recipient), false),
                AccountMeta::new(ata(creator_vault), false),
                AccountMeta::new_readonly(creator_vault, false),
                AccountMeta::new_readonly(pda(&[b"global_volume_accumulator"]), false),
                AccountMeta::new(volume, false),
                AccountMeta::new_readonly(fee_config, false),
                AccountMeta::new_readonly(PUMP_FEE, false),
            ]);
            if cashback {
                accounts.extend([
                    AccountMeta::new(ata(volume), false),
                    AccountMeta::new(volume, false),
                ]);
            }
            if creator != Pubkey::default() {
                accounts.push(AccountMeta::new_readonly(
                    pda(&[b"pool-v2", mint0.as_ref()]),
                    false,
                ));
            }
            accounts.extend([
                AccountMeta::new_readonly(buyback, false),
                AccountMeta::new(ata(buyback), false),
            ]);
            Venue::Pump
        };
        Ok(Self {
            venue,
            pool,
            x_mint: x.mint,
            base_mint: base.mint,
            accounts,
            pump_quote_is_base: Some(quote.mint == base.mint),
            whirlpool_adaptive_fee: None,
        })
    }

    pub fn supports_meteora_state(data: &[u8]) -> bool {
        data.get(233) == Some(&1) && data.get(874) == Some(&0)
    }

    /// Meteora DAMM v1 reads each vault's token account and LP mint from the
    /// vault-program state, so both vault accounts accompany the pool state.
    pub fn from_meteora_state(
        pool: Pubkey,
        data: &[u8],
        a_vault: &[u8],
        b_vault: &[u8],
        x: MintAccounts,
        base: MintAccounts,
    ) -> Result<Self> {
        ensure!(
            x.token_program == TOKEN && base.token_program == TOKEN,
            "V10 Meteora DAMM v1 requires legacy token mints"
        );
        ensure!(
            Self::supports_meteora_state(data),
            "V10 Meteora DAMM v1 requires an enabled constant-product pool"
        );
        let pair = PoolPair::read(METEORA, data)?;
        let base_is_a = if pair.mints == [base.mint, x.mint] {
            true
        } else if pair.mints == [x.mint, base.mint] {
            false
        } else {
            bail!("pool does not match the declared mint pair");
        };
        let key =
            |d: &[u8], offset| -> Result<Pubkey> { Ok(Pubkey::new_from_array(read(d, offset)?)) };
        // Per token: vault, token vault, LP mint, pool LP account, admin fee account.
        let side = |state: &[u8], index: usize, pool_lp, fee| -> Result<[Pubkey; 5]> {
            ensure!(
                key(state, 83)? == pair.mints[index],
                "Meteora vault mint mismatch"
            );
            Ok([
                pair.vaults[index],
                key(state, 19)?,
                key(state, 115)?,
                key(data, pool_lp)?,
                key(data, fee)?,
            ])
        };
        let a = side(a_vault, 0, 168, 234)?;
        let b = side(b_vault, 1, 200, 266)?;
        let (x_side, base_side) = if base_is_a { (b, a) } else { (a, b) };
        Self::from_meteora_keys(pool, x, base, x_side, base_side)
    }

    /// Each side is vault, token vault, LP mint, pool LP account, admin fee account.
    pub fn from_meteora_keys(
        pool: Pubkey,
        x: MintAccounts,
        base: MintAccounts,
        x_side: [Pubkey; 5],
        base_side: [Pubkey; 5],
    ) -> Result<Self> {
        ensure!(
            x.token_program == TOKEN && base.token_program == TOKEN,
            "V10 Meteora DAMM v1 requires legacy token mints"
        );
        let mut accounts = vec![
            AccountMeta::new_readonly(METEORA, false),
            AccountMeta::new_readonly(base.mint, false),
            AccountMeta::new_readonly(METEORA_VAULT, false),
            AccountMeta::new(pool, false),
        ];
        accounts.extend(
            x_side
                .into_iter()
                .zip(base_side)
                .flat_map(|(x, base)| [AccountMeta::new(x, false), AccountMeta::new(base, false)]),
        );
        Ok(Self {
            venue: Venue::MeteoraDamm,
            pool,
            x_mint: x.mint,
            base_mint: base.mint,
            accounts,
            pump_quote_is_base: None,
            whirlpool_adaptive_fee: None,
        })
    }

    /// An Archer market's block from its state and the maker books to list,
    /// in the order the aggregator splits tied prices.
    pub fn from_archer_state(
        pool: Pubkey,
        data: &[u8],
        x: MintAccounts,
        base: MintAccounts,
        books: &[Pubkey],
    ) -> Result<Self> {
        archer::check_market(&pool, data)?;
        let vaults = PoolPair::read(ARCHER, data)?.vaults_for(x.mint, base.mint)?;
        Self::from_keys(pool, x, base, vaults, PoolKeys::Archer { books })
    }

    /// The conversions of an issuer, Hylo or a stake pool, at `pool`, which
    /// `program` performs with `block` after the program and declared base
    /// mint.
    fn from_issuer(
        program: Pubkey,
        pool: Pubkey,
        block: Vec<AccountMeta>,
        venue: Venue,
        x: MintAccounts,
        base: MintAccounts,
    ) -> Result<Self> {
        // Hylo's handlers take one token program for both mints, and the
        // stake pools the executor trades issue legacy SPL tokens for WSOL.
        ensure!(
            x.token_program == TOKEN && base.token_program == TOKEN,
            "an issuer's mints are legacy SPL tokens"
        );
        let mut accounts = vec![
            AccountMeta::new_readonly(program, false),
            AccountMeta::new_readonly(base.mint, false),
        ];
        accounts.extend(block);
        Ok(Self {
            venue,
            pool,
            x_mint: x.mint,
            base_mint: base.mint,
            accounts,
            pump_quote_is_base: None,
            whirlpool_adaptive_fee: None,
        })
    }

    /// A Manifest market's block listing the global account and vault of
    /// each of `mints`, the market's own, whose global orders the executor
    /// then fills.
    pub fn with_manifest_globals(mut self, mints: &[Pubkey]) -> Result<Self> {
        ensure!(
            self.venue == Venue::Manifest && self.accounts.len() == MANIFEST_POOL_ACCOUNTS,
            "only a Manifest market without globals takes them"
        );
        ensure!(
            mints
                .iter()
                .all(|mint| [self.x_mint, self.base_mint].contains(mint)),
            "a Manifest market's globals are of its own mints"
        );
        ensure!(
            mints.len() < 2 || (mints.len() == 2 && mints[0] != mints[1]),
            "a Manifest market takes one global per mint"
        );
        for mint in mints {
            self.accounts
                .extend(manifest_global(mint).map(|key| AccountMeta::new(key, false)));
        }
        if !mints.is_empty() {
            self.venue = Venue::ManifestGlobal;
        }
        Ok(self)
    }

    pub fn venue(&self) -> Venue {
        self.venue
    }
    pub fn pump_quote_is_base(&self) -> Option<bool> {
        self.pump_quote_is_base
    }
    pub fn whirlpool_adaptive_fee(&self) -> Option<bool> {
        self.whirlpool_adaptive_fee
    }
    pub fn pool(&self) -> Pubkey {
        self.pool
    }
    pub fn accounts(&self) -> &[AccountMeta] {
        &self.accounts
    }
    pub fn x_mint(&self) -> Pubkey {
        self.x_mint
    }
    pub fn base_mint(&self) -> Pubkey {
        self.base_mint
    }

    /// The state and arrays must come from the same account snapshot. Pool mints determine orientation.
    pub fn from_state(
        pool: Pubkey,
        owner: Pubkey,
        data: &[u8],
        x: MintAccounts,
        base: MintAccounts,
        arrays: &[Pubkey],
        bitmap: Option<Pubkey>,
    ) -> Result<Self> {
        let key = |offset: usize| -> Result<Pubkey> {
            Ok(Pubkey::new_from_array(
                data.get(offset..offset + 32)
                    .ok_or_else(|| anyhow::anyhow!("truncated pool {pool}"))?
                    .try_into()?,
            ))
        };
        ensure!(
            owner != PUMP,
            "Pump requires from_pump_keys with its fee accounts"
        );
        ensure!(
            owner != METEORA,
            "Meteora DAMM v1 requires from_meteora_state with its vault states"
        );
        ensure!(
            owner != ARCHER,
            "Archer requires from_archer_state with its maker books"
        );
        if !uses_arrays(owner) {
            ensure!(
                arrays.is_empty() && bitmap.is_none(),
                "pool does not use arrays"
            );
        }
        if is_hylo(&owner) {
            let hylo = HyloPool::read(&owner, data, [x.mint, base.mint])?;
            ensure!(pool == hylo.pool, "{pool} is not Hylo's {}", hylo.pool);
            let block = hylo.accounts().collect();
            return Self::from_issuer(owner, pool, block, hylo.venue(), x, base);
        }
        if owner == SPL_STAKE_POOL {
            let block = StakePool::read_trading(data, x, base)?.accounts(pool)?;
            return Self::from_issuer(owner, pool, block.to_vec(), Venue::StakePool, x, base);
        }
        let [x_vault, base_vault] = PoolPair::read(owner, data)?.vaults_for(x.mint, base.mint)?;
        let keys = match owner {
            RAYDIUM => PoolKeys::Raydium,
            CPMM => PoolKeys::Cpmm {
                config: key(8)?,
                observation: key(296)?,
            },
            DAMMV2 => PoolKeys::DammV2 {
                compounding: read::<1>(data, DAMMV2_COLLECT_FEE_MODE_OFFSET)?[0]
                    == DAMMV2_COMPOUNDING_FEE_MODE,
            },
            MANIFEST => {
                ensure!(
                    ManifestBook(data).has_spare_node(),
                    "Manifest market {pool} has no spare order node"
                );
                PoolKeys::Manifest
            }
            FUTARCHY => PoolKeys::Futarchy,
            CLMM | PANCAKESWAP | BYREAL => PoolKeys::Clmm {
                program: owner,
                config: key(9)?,
                observation: key(201)?,
                bitmap: bitmap.ok_or_else(|| anyhow::anyhow!("CLMM bitmap address required"))?,
                arrays,
                pyth: if owner == BYREAL {
                    byreal_price_updates(data)?.map(|updates| updates.map(|update| update.account))
                } else {
                    None
                },
            },
            WHIRLPOOL => {
                ensure!(bitmap.is_none(), "Whirlpool has no bitmap extension");
                PoolKeys::Whirlpool {
                    arrays,
                    adaptive_fee: Some(whirlpool_state_adaptive_fee(data)?),
                }
            }
            DLMM => PoolKeys::Dlmm {
                oracle: key(552)?,
                bitmap,
                arrays,
            },
            DLMM_PRO => {
                dlmm_pro::check_tradable(&pool, data)?;
                ensure!(
                    bitmap == dlmm_pro::flagged_bitmap_extension(&pool, data),
                    "DLMM Pro bitmap extension does not match its pool flag"
                );
                PoolKeys::DlmmPro { bitmap, arrays }
            }
            _ => bail!("unsupported V10 pool program"),
        };
        Self::from_keys(pool, x, base, [x_vault, base_vault], keys)
    }

    /// Encode decoded pool metadata using the same layout as `from_state`.
    pub fn from_keys(
        pool: Pubkey,
        x: MintAccounts,
        base: MintAccounts,
        vaults: [Pubkey; 2],
        keys: PoolKeys<'_>,
    ) -> Result<Self> {
        ensure!(x.mint != base.mint, "pool mints must differ");
        let owner = keys.program();
        let [x_vault, base_vault] = vaults;
        let mut accounts = vec![
            AccountMeta::new_readonly(owner, false),
            AccountMeta::new_readonly(base.mint, false),
        ];
        match keys {
            PoolKeys::Manifest => {
                accounts.extend([
                    AccountMeta::new(pool, false),
                    AccountMeta::new(x_vault, false),
                    AccountMeta::new(base_vault, false),
                ]);
            }
            PoolKeys::Futarchy => {
                // `spot_swap` takes one token program for both mints.
                ensure!(
                    !uses_token_2022(x, base),
                    "V10 Futarchy requires legacy token mints"
                );
                accounts.extend([
                    AccountMeta::new(pool, false),
                    AccountMeta::new(x_vault, false),
                    AccountMeta::new(base_vault, false),
                    AccountMeta::new_readonly(
                        Pubkey::find_program_address(&[b"__event_authority"], &owner).0,
                        false,
                    ),
                ]);
            }
            PoolKeys::DlmmPro { bitmap, arrays } => {
                ensure!(
                    !arrays.is_empty() && arrays.len() <= MAX_ARRAYS,
                    "V10 requires 1..={MAX_ARRAYS} arrays per DLMM Pro pool"
                );
                accounts.extend([
                    AccountMeta::new(pool, false),
                    AccountMeta::new(x_vault, false),
                    AccountMeta::new(base_vault, false),
                    AccountMeta::new(dlmm_pro::oracle(&pool), false),
                    AccountMeta::new_readonly(DLMM_PRO_POOL_AUTHORITY, false),
                    AccountMeta::new_readonly(DLMM_PRO_EVENT_AUTHORITY, false),
                    // The executor passes it to every swap, whatever the
                    // mints' token programs.
                    AccountMeta::new_readonly(MEMO, false),
                ]);
                accounts.extend(bitmap.map(|key| AccountMeta::new(key, false)));
                accounts.extend(arrays.iter().map(|&a| AccountMeta::new(a, false)));
            }
            PoolKeys::Archer { books } => {
                ensure!(
                    (1..=ARCHER_MAX_BOOKS).contains(&books.len()),
                    "V10 requires 1..={ARCHER_MAX_BOOKS} books per Archer market"
                );
                for (index, book) in books.iter().enumerate() {
                    ensure!(
                        ![pool, x_vault, base_vault].contains(book)
                            && !books[..index].contains(book),
                        "Archer book {book} repeats a listed account"
                    );
                }
                accounts.extend([
                    AccountMeta::new(pool, false),
                    AccountMeta::new(x_vault, false),
                    AccountMeta::new(base_vault, false),
                ]);
                accounts.extend(books.iter().map(|&book| AccountMeta::new(book, false)));
            }
            PoolKeys::Raydium => {
                ensure!(
                    !uses_token_2022(x, base),
                    "V10 Raydium requires legacy token mints"
                );
                accounts.extend([
                    AccountMeta::new_readonly(
                        Pubkey::find_program_address(&[b"amm authority"], &owner).0,
                        false,
                    ),
                    AccountMeta::new(pool, false),
                    AccountMeta::new(x_vault, false),
                    AccountMeta::new(base_vault, false),
                ]);
            }
            PoolKeys::DammV2 { .. } => {
                accounts.extend([
                    AccountMeta::new_readonly(
                        Pubkey::find_program_address(&[b"__event_authority"], &owner).0,
                        false,
                    ),
                    AccountMeta::new_readonly(
                        Pubkey::find_program_address(&[b"pool_authority"], &owner).0,
                        false,
                    ),
                    AccountMeta::new(pool, false),
                    AccountMeta::new(x_vault, false),
                    AccountMeta::new(base_vault, false),
                ]);
            }
            PoolKeys::Cpmm {
                config,
                observation,
            } => {
                accounts.extend([
                    AccountMeta::new_readonly(
                        Pubkey::find_program_address(&[b"vault_and_lp_mint_auth_seed"], &CPMM).0,
                        false,
                    ),
                    AccountMeta::new(pool, false),
                    AccountMeta::new_readonly(config, false),
                    AccountMeta::new(x_vault, false),
                    AccountMeta::new(base_vault, false),
                    AccountMeta::new(observation, false),
                ]);
            }
            PoolKeys::Clmm { arrays, .. }
            | PoolKeys::Whirlpool { arrays, .. }
            | PoolKeys::Dlmm { arrays, .. } => {
                ensure!(
                    !arrays.is_empty() && arrays.len() <= MAX_ARRAYS,
                    "V10 requires 1..={MAX_ARRAYS} arrays per concentrated pool"
                );
                let pyth = match keys {
                    PoolKeys::Clmm { pyth, .. } => pyth,
                    _ => None,
                };
                if owner == DLMM {
                    accounts.push(AccountMeta::new_readonly(
                        Pubkey::find_program_address(&[b"__event_authority"], &DLMM).0,
                        false,
                    ));
                }
                if lists_memo(owner, x, base, pyth.is_some()) {
                    accounts.push(AccountMeta::new_readonly(MEMO, false));
                }
                accounts.push(AccountMeta::new(pool, false));
                match keys {
                    PoolKeys::Clmm {
                        program,
                        config,
                        observation,
                        bitmap,
                        ..
                    } => {
                        ensure!(
                            [CLMM, PANCAKESWAP, BYREAL].contains(&program),
                            "unsupported CLMM program"
                        );
                        ensure!(
                            pyth.is_none() || program == BYREAL,
                            "only Byreal pools take Pyth price updates"
                        );
                        accounts.extend([
                            AccountMeta::new_readonly(config, false),
                            AccountMeta::new(observation, false),
                            AccountMeta::new(bitmap, false),
                            AccountMeta::new(x_vault, false),
                            AccountMeta::new(base_vault, false),
                        ]);
                    }
                    PoolKeys::Whirlpool { .. } => {
                        let oracle =
                            Pubkey::find_program_address(&[b"oracle", pool.as_ref()], &WHIRLPOOL).0;
                        accounts.extend([
                            AccountMeta::new(oracle, false),
                            AccountMeta::new(x_vault, false),
                            AccountMeta::new(base_vault, false),
                        ]);
                    }
                    PoolKeys::Dlmm { oracle, bitmap, .. } => {
                        accounts.extend([
                            AccountMeta::new(x_vault, false),
                            AccountMeta::new(base_vault, false),
                            AccountMeta::new(oracle, false),
                        ]);
                        accounts.extend(bitmap.map(|bitmap| AccountMeta::new(bitmap, false)));
                    }
                    _ => unreachable!(),
                }
                accounts.extend(arrays.iter().map(|&a| AccountMeta::new(a, false)));
                if let Some(updates) = pyth {
                    // swap_v3_dyn requires the Token-2022 program even for
                    // standard mints.
                    accounts.push(AccountMeta::new_readonly(TOKEN_2022, false));
                    accounts.extend(updates.map(|update| AccountMeta::new_readonly(update, false)));
                }
            }
        }
        Ok(Self {
            venue: keys.venue(),
            pool,
            x_mint: x.mint,
            base_mint: base.mint,
            accounts,
            pump_quote_is_base: None,
            whirlpool_adaptive_fee: match keys {
                PoolKeys::Whirlpool { adaptive_fee, .. } => adaptive_fee,
                _ => None,
            },
        })
    }
}

#[derive(Clone, Debug)]
pub struct MarketGroup {
    pub target: MintAccounts,
    pub base: MintAccounts,
    pub intermediate: PoolAccounts,
    pub bridges: Vec<PoolAccounts>,
    pub direct: Vec<PoolAccounts>,
}

#[derive(Clone, Debug)]
pub struct DirectGroup {
    pub target: MintAccounts,
    pub pools: Vec<PoolAccounts>,
}

#[derive(Clone, Debug)]
pub struct ConversionAccounts {
    pub quote: MintAccounts,
    pub pool: PoolAccounts,
}

#[derive(Clone, Debug)]
pub struct V10Instruction {
    pub program_id: Pubkey,
    pub wallet: Pubkey,
    pub settlement: MintAccounts,
    pub conversion: Option<ConversionAccounts>,
    pub fee_collector: Pubkey,
    pub additional_fee_collector: Option<Pubkey>,
    pub flashloan: Option<(Pubkey, Pubkey)>,
    pub header: Header,
    pub groups: Vec<MarketGroup>,
}
impl V10Instruction {
    pub fn build(&self) -> Result<Instruction> {
        self.build_groups(None)
    }

    /// Direct groups only; the request's triangle groups must be empty.
    pub fn build_direct(&self, groups: &[DirectGroup]) -> Result<Instruction> {
        ensure!(
            self.groups.is_empty(),
            "direct and triangle groups cannot be combined"
        );
        self.build_groups(Some(groups))
    }

    fn build_groups(&self, direct: Option<&[DirectGroup]>) -> Result<Instruction> {
        ensure!(
            [SOL, USDC].contains(&self.settlement.mint),
            "settlement must be WSOL or USDC"
        );
        ensure!(
            self.settlement.token_program == TOKEN,
            "invalid settlement token program"
        );
        ensure!(
            self.header.use_flashloan == self.flashloan.is_some(),
            "flashloan header/accounts mismatch"
        );
        ensure!(
            (self.header.additional_fee_bp > 0) == self.additional_fee_collector.is_some(),
            "additional fee header/accounts mismatch"
        );
        let group_count = direct.map_or(self.groups.len(), <[DirectGroup]>::len);
        ensure!(
            group_count > 0 && group_count <= MAX_GROUPS,
            "invalid V10 group count"
        );
        let mut data = InstructionData {
            header: self.header,
            group_count: group_count as u8,
            ..InstructionData::default()
        };
        let mut accounts = vec![
            AccountMeta::new(self.wallet, true),
            AccountMeta::new_readonly(self.settlement.mint, false),
            AccountMeta::new(self.fee_collector, false),
        ];
        if let Some(fee) = self.additional_fee_collector {
            accounts.push(AccountMeta::new(fee, false));
        }
        accounts.extend([
            AccountMeta::new(self.settlement.wallet, false),
            AccountMeta::new_readonly(TOKEN, false),
            AccountMeta::new_readonly(SYSTEM, false),
            AccountMeta::new_readonly(ASSOCIATED, false),
        ]);
        if let Some((authority, vault)) = self.flashloan {
            accounts.extend([
                AccountMeta::new_readonly(authority, false),
                AccountMeta::new(vault, false),
            ]);
        }
        let quote = if let Some(c) = &self.conversion {
            ensure!(
                [SOL, USDC].contains(&c.quote.mint)
                    && c.quote.mint != self.settlement.mint
                    && c.quote.token_program == TOKEN,
                "conversion must connect WSOL and USDC"
            );
            ensure!(
                c.pool.x_mint == c.quote.mint && c.pool.base_mint == self.settlement.mint,
                "invalid settlement conversion pool"
            );
            data.conversion_account_count = c.pool.accounts.len().try_into()?;
            c.quote.append(&mut accounts);
            accounts.extend_from_slice(&c.pool.accounts);
            c.quote
        } else {
            self.settlement
        };
        let mut total = usize::from(self.conversion.is_some());
        for (i, group) in direct.unwrap_or_default().iter().enumerate() {
            ensure!(
                group.target.mint != self.settlement.mint && group.target.mint != quote.mint,
                "direct target must differ from settlement and conversion quote"
            );
            ensure!(
                group.pools.len() >= 2,
                "direct groups require at least two pools"
            );
            ensure!(
                group.pools.iter().any(|p| p.base_mint == self.settlement.mint),
                "direct groups currently require a settlement-quoted pool; repeated conversion is not implemented"
            );
            total += group.pools.len();
            ensure!(total <= MAX_POOLS, "too many V10 pools");
            group.target.append(&mut accounts);
            let desc = &mut data.groups[i];
            desc.direct_count = group.pools.len() as u8;
            for (j, pool) in group.pools.iter().enumerate() {
                ensure!(
                    pool.x_mint == group.target.mint
                        && [self.settlement.mint, quote.mint].contains(&pool.base_mint),
                    "incorrect direct pool mint pair"
                );
                desc.pool_account_counts[j] = pool.accounts.len().try_into()?;
                accounts.extend_from_slice(&pool.accounts);
            }
        }
        for (i, g) in self.groups.iter().enumerate() {
            ensure!(
                g.target.mint != g.base.mint
                    && ![g.target.mint, g.base.mint].contains(&self.settlement.mint)
                    && ![g.target.mint, g.base.mint].contains(&quote.mint),
                "triangle mints must differ"
            );
            ensure!(
                !g.bridges.is_empty() && !g.direct.is_empty(),
                "bridges and direct candidates required"
            );
            let count = 1 + g.bridges.len() + g.direct.len();
            total += count;
            ensure!(
                total <= MAX_POOLS,
                "V10 permits at most {MAX_POOLS} pool declarations"
            );
            let mut desc = Group {
                bridge_count: g.bridges.len() as u8,
                direct_count: g.direct.len() as u8,
                ..Group::default()
            };
            g.target.append(&mut accounts);
            g.base.append(&mut accounts);
            for (j, p) in core::iter::once(&g.intermediate)
                .chain(g.bridges.iter())
                .chain(g.direct.iter())
                .enumerate()
            {
                let (x, b) = if j == 0 {
                    (g.target.mint, g.base.mint)
                } else if j <= g.bridges.len() {
                    (g.base.mint, quote.mint)
                } else {
                    (g.target.mint, self.settlement.mint)
                };
                ensure!(
                    p.x_mint == x && p.base_mint == b,
                    "pool {} has incorrect V10 role",
                    p.pool
                );
                desc.pool_account_counts[j] = p.accounts.len().try_into()?;
                accounts.extend_from_slice(&p.accounts);
            }
            data.groups[i] = desc;
        }
        let mut buffer = [0; MAX_PAYLOAD_LEN];
        let payload = data
            .encode(&mut buffer)
            .map_err(|_| anyhow::anyhow!("invalid V10 payload bounds"))?;
        let mut bytes = vec![OPCODE];
        bytes.extend_from_slice(payload);
        ensure!(
            accounts.len() == data.account_count(),
            "V10 account count mismatch"
        );
        Ok(Instruction {
            program_id: self.program_id,
            accounts,
            data: bytes,
        })
    }
}
