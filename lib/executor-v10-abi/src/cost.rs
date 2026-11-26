//! The executor's CU cost model: per-venue costs measured on SVM replays,
//! and the fixed work around them. The program budgets its search with
//! these, and clients size a transaction's limit with the same numbers.
//!
//! Every search charge is the work the native integer search measurably
//! performs for that operation, with a margin of about a quarter over its
//! median; execution reserves cover the swap programs' own cost near their
//! 99th percentile, the executor's invoke work per leg, and a safety margin.

/// A pool venue the executor can trade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Venue {
    Manifest,
    /// A Manifest market whose block lists a global account, whose global
    /// orders the executor fills.
    ManifestGlobal,
    RaydiumAmm,
    MeteoraDamm,
    Pump,
    /// A Pump pool the executor trades with PumpSwap's v2 instructions.
    PumpV2,
    RaydiumCpmm,
    Clmm,
    /// PancakeSwap and Byreal, Raydium CLMM forks with costlier swaps.
    ClmmFork,
    /// A Byreal pool with swap dynamic fees, traded with `swap_v3_dyn`.
    ByrealDynamicFee,
    Whirlpool,
    Dlmm,
    /// A DAMM v2 pool that collects its fees apart from its liquidity.
    Dammv2,
    /// A DAMM v2 pool that compounds its fees into its reserves
    /// (`collect_fee_mode` 2), whose swap does more work.
    Dammv2Compounding,
    /// A Futarchy Dao's spot pool.
    Futarchy,
    /// Hylo's USDC pair: hyUSD minted against USDC, or redeemed for it.
    HyloUsdc,
    /// Hylo's Stability Pool: hyUSD deposited for sHYUSD, or withdrawn.
    HyloStability,
    /// hyUSD minted against a liquid-staking token Hylo holds, or redeemed
    /// for it.
    HyloLst,
    /// hyUSD minted against an Exo collateral, or redeemed for it.
    HyloExo,
    /// xSOL, Hylo's levercoin, minted against a liquid-staking token Hylo
    /// holds, or redeemed for it.
    HyloLstLever,
    /// An Exo pair's levercoin minted against its collateral, or redeemed
    /// for it.
    HyloExoLever,
    /// hyUSD converted to xSOL, or xSOL to hyUSD.
    HyloLstConvert,
    /// hyUSD converted to an Exo pair's levercoin, or the levercoin to hyUSD.
    HyloExoConvert,
    /// A Meteora DLMM Pro pool: concentrated liquidity whose ticks can also
    /// hold limit orders.
    DlmmPro,
    /// An SPL stake pool's SOL deposit or withdrawal, traded against WSOL.
    StakePool,
    /// An Archer market's maker books, which the swap fills by price
    /// priority and splits pro rata at tied prices.
    Archer,
}

/// One venue's work. `select` parses, validates and quotes a pool before the
/// search starts, and enumerates its routes. Two-leg solvers charge
/// `interval` per combined iteration; longer routes charge `load` for every
/// segment and `search_crossing` for every one past the first, and `bound`
/// for each pass of the walk's stop check. `swap` reserves the swap program's
/// execution and the executor's invoke work around it; `crossing` each tick,
/// bin or level the swap crosses, or for Archer each maker book's fill after
/// the first; `token_2022` is the swap's extra cost per Token-2022 mint it
/// transfers. The `select` of Hylo's venues, the stake pool and DLMM Pro is
/// CPMM's scaled by the parsing and pricing work measured for each, rounded
/// up to the hundred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VenueCost {
    pub select: u32,
    pub prepare: u32,
    pub interval: u32,
    pub load: u32,
    pub search_crossing: u32,
    pub bound: u32,
    pub swap: u32,
    pub crossing: u32,
    pub token_2022: u32,
}

impl Venue {
    /// Inlined, so the program's reads reduce to the fields it uses.
    #[inline(always)]
    pub const fn cost(self) -> VenueCost {
        match self {
            Self::Manifest => VenueCost {
                select: 2_500,
                prepare: 800,
                interval: 1_500,
                load: 600,
                search_crossing: 1_000,
                bound: 200,
                swap: 16_900,
                crossing: 2_000,
                token_2022: 8_000,
            },
            // The global's single transfer to the market and passing its
            // accounts add 1,500 to a swap; each global fill about 1,200.
            Self::ManifestGlobal => VenueCost {
                select: 2_500,
                prepare: 800,
                interval: 1_500,
                load: 600,
                search_crossing: 1_000,
                bound: 200,
                swap: 19_000,
                crossing: 3_500,
                token_2022: 8_000,
            },
            Self::RaydiumAmm => VenueCost {
                select: 3_300,
                prepare: 1_000,
                interval: 1_300,
                load: 500,
                search_crossing: 0,
                bound: 150,
                swap: 18_600,
                crossing: 0,
                token_2022: 0,
            },
            Self::MeteoraDamm => VenueCost {
                select: 8_000,
                prepare: 1_000,
                interval: 2_000,
                load: 500,
                search_crossing: 0,
                bound: 150,
                swap: 81_500,
                crossing: 0,
                token_2022: 3_000,
            },
            Self::Pump => VenueCost {
                select: 3_200,
                prepare: 300,
                interval: 1_200,
                load: 500,
                search_crossing: 0,
                bound: 150,
                swap: 82_700,
                crossing: 0,
                token_2022: 3_000,
            },
            // V2 quotes like v1. Its swap and Token-2022 reserves keep the
            // costliest replayed v2 sell of each mint class at least 4,300
            // CU below its reserve, after 2,000 CU of the executor's invoke
            // work. V2 does more of its own work per Token-2022 mint than the
            // transfer alone.
            Self::PumpV2 => VenueCost {
                select: 3_200,
                prepare: 300,
                interval: 1_200,
                load: 500,
                search_crossing: 0,
                bound: 150,
                swap: 56_100,
                crossing: 0,
                token_2022: 7_500,
            },
            Self::RaydiumCpmm => VenueCost {
                select: 3_300,
                prepare: 1_300,
                interval: 1_200,
                load: 500,
                search_crossing: 0,
                bound: 150,
                swap: 27_300,
                crossing: 0,
                token_2022: 8_500,
            },
            Self::Clmm => CLMM_COST,
            Self::ClmmFork => VenueCost {
                select: 5_000,
                prepare: 4_000,
                interval: 3_000,
                load: 1_500,
                search_crossing: 3_000,
                bound: 300,
                swap: 59_500,
                crossing: 10_000,
                token_2022: 8_000,
            },
            // ClmmFork's costs plus the swap-dynamic-fee work, each surcharge
            // at least a quarter over the most that same-state pairs of plain
            // and swap_v3_dyn swaps measured against the deployed program:
            // 2,796 executor CU (price checks and the fee) and 31,580 Byreal
            // CU. The pairs cross no ticks; Byreal prices the fee once per
            // swap.
            Self::ByrealDynamicFee => VenueCost {
                select: 8_600,
                prepare: 4_000,
                interval: 3_000,
                load: 1_500,
                search_crossing: 3_000,
                bound: 300,
                swap: 99_000,
                crossing: 10_000,
                token_2022: 8_000,
            },
            Self::Whirlpool => VenueCost {
                select: 4_300,
                prepare: 6_500,
                interval: 4_500,
                load: 1_500,
                search_crossing: 2_500,
                bound: 300,
                swap: 41_500,
                crossing: 7_100,
                token_2022: 9_900,
            },
            Self::Dlmm => VenueCost {
                select: 5_000,
                prepare: 2_300,
                interval: 1_300,
                load: 600,
                search_crossing: 1_200,
                bound: 600,
                swap: 35_400,
                // An upper bound per funded bin, not a typical cost: the
                // native swap's per-bin fee, price and amount arithmetic
                // grows with operand width and fee terms, measured up to
                // 6,703. A typical bin's 4-6k would let long walks overrun.
                crossing: 6_800,
                token_2022: 5_600,
            },
            // The costliest replayed swaps needed 20,255 and, compounding,
            // 29,116 CU beyond their Token-2022 mints and referral claim,
            // with about 2,000 CU of the executor's invoke work.
            Self::Dammv2 => VenueCost {
                swap: 21_000,
                ..DAMMV2_COST
            },
            Self::Dammv2Compounding => VenueCost {
                swap: 29_500,
                ..DAMMV2_COST
            },
            // A constant-product pool the search prices like Raydium's.
            // Its `spot_swap` cost 38,506 to 38,602 CU in 38 replayed swaps
            // of one Dao; it transfers only legacy token mints.
            Self::Futarchy => VenueCost {
                select: 3_300,
                prepare: 1_000,
                interval: 1_300,
                load: 500,
                search_crossing: 0,
                bound: 150,
                swap: 42_000,
                crossing: 0,
                token_2022: 0,
            },
            // The swap is the 99th percentile of the costlier direction's
            // mainnet handler cost, a redemption or a withdrawal, plus the
            // executor's invoke work, rounded up to the hundred.
            Self::HyloUsdc => VenueCost {
                select: 2_900,
                swap: 64_000,
                ..ISSUER_COST
            },
            Self::HyloStability => VenueCost {
                select: 3_100,
                swap: 46_400,
                ..ISSUER_COST
            },
            // As above, except that the costlier direction is a mint, which
            // no sampled mainnet transaction made: its cost is the costliest
            // direct mint from a recorded state under LiteSVM.
            Self::HyloLst => VenueCost {
                select: 9_500,
                swap: 61_500,
                ..ISSUER_COST
            },
            Self::HyloExo => VenueCost {
                select: 9_700,
                swap: 59_900,
                ..ISSUER_COST
            },
            // As for the USDC pair, the swap is the 99th percentile of the
            // costlier direction's mainnet handler cost plus the executor's
            // invoke work: a levercoin mint, or a conversion from hyUSD to
            // levercoins. Each swap exceeds the costliest direct trade of
            // that venue from a recorded state under LiteSVM.
            Self::HyloLstLever => VenueCost {
                select: 11_700,
                swap: 60_900,
                ..ISSUER_COST
            },
            Self::HyloExoLever => VenueCost {
                select: 11_900,
                swap: 58_100,
                ..ISSUER_COST
            },
            Self::HyloLstConvert => VenueCost {
                select: 7_900,
                swap: 58_100,
                ..ISSUER_COST
            },
            Self::HyloExoConvert => VenueCost {
                select: 8_500,
                swap: 72_500,
                ..ISSUER_COST
            },
            // The search charges are Raydium CLMM's, which cover DLMM Pro's
            // measured estimates with more margin than CPMM's charges
            // cover its own. The swap is the costliest sampled mainnet swap
            // of legacy mints that crossed no tick plus 2,000 for the
            // executor's invoke work, the crossing the steepest per-tick cost
            // fitted on the recorded pools, and a Token-2022 mint a quarter
            // over the most that a mint's conversion and an active transfer
            // fee added to same-input swaps. Each is rounded up to the
            // hundred.
            Self::DlmmPro => VenueCost {
                select: 4_900,
                swap: 38_600,
                crossing: 4_200,
                token_2022: 3_900,
                ..CLMM_COST
            },
            // The swap covers the costlier direction, a withdrawal with its
            // SyncNative and surplus return: its costliest leg under LiteSVM,
            // and the costliest sampled mainnet handler plus the most a leg
            // cost beyond its handler, rounded up to the hundred.
            Self::StakePool => VenueCost {
                select: 4_200,
                swap: 26_700,
                ..ISSUER_COST
            },
            // The search charges are Manifest's. The swap covers a one-book
            // fill, `ARCHER_BOOK_CU` each listed book's admission, and
            // `crossing` each book's fill after the first, whether at a new
            // price or a tied one.
            Self::Archer => VenueCost {
                select: 2_500,
                prepare: 800,
                interval: 1_500,
                load: 600,
                search_crossing: 1_000,
                bound: 200,
                swap: 30_000,
                crossing: 4_000,
                token_2022: 8_000,
            },
        }
    }

    pub const fn is_pump(self) -> bool {
        matches!(self, Self::Pump | Self::PumpV2)
    }

    /// The extra a Pump buy reserves over its sell. Other venues have none.
    pub const fn pump_buy_cu(self) -> u32 {
        match self {
            Self::Pump => PUMP_BUY_CU,
            Self::PumpV2 => PUMP_V2_BUY_CU,
            _ => 0,
        }
    }
}

const CLMM_COST: VenueCost = VenueCost {
    select: 4_300,
    prepare: 5_000,
    interval: 3_000,
    load: 1_500,
    search_crossing: 3_000,
    bound: 300,
    swap: 46_500,
    crossing: 10_000,
    token_2022: 8_000,
};

/// An issuer's work apart from its selection and swap: Hylo's venues and
/// the stake pool. The search walks each direction of an issuer as one
/// linear segment at a rate bounding its quote from above, and issuers
/// transfer only legacy token mints. Fitted against Raydium CPMM's charges in
/// the same baskets of Hylo trades.
const ISSUER_COST: VenueCost = VenueCost {
    select: 0,
    prepare: 500,
    interval: 1_000,
    load: 500,
    search_crossing: 0,
    bound: 150,
    swap: 0,
    crossing: 0,
    token_2022: 0,
};

/// DAMM v2's work apart from its swap, which its pool's fee collection sets.
const DAMMV2_COST: VenueCost = VenueCost {
    select: 3_000,
    prepare: 800,
    interval: 1_300,
    load: 500,
    search_crossing: 0,
    bound: 150,
    swap: 0,
    crossing: 0,
    token_2022: 6_000,
};

/// Entry, instruction decoding, the common accounts and the brand log before
/// any pool is parsed, and each group's target mint and descriptor.
const SETUP_CU: u32 = 3_500;
const GROUP_SETUP_CU: u32 = 1_500;
/// Parsing a Token-2022 mint walks its extensions.
pub const TOKEN_2022_MINT_CU: u32 = 1_500;
/// Validating a wallet token account that does not exist yet derives its
/// address; each rejected bump is another derivation.
pub const MISSING_WALLET_PARSE_CU: u32 = 2_000;
/// A Pump pool without a stored coin creator derives the pool authority.
pub const PUMP_PDA_CU: u32 = 1_600;
/// Quoting work beyond each pool's `select`, such as bin and tick scans,
/// which the program measures as it goes.
const DISCOVERY_CU: u32 = 6_000;
/// Listing a route that passes the fee screen: its canonical key, ranking
/// edge and potential, and bookkeeping. About 1,300 CU measured.
pub const LISTED_ROUTE_CU: u32 = 1_500;
/// Screening a route out, or dropping an alias or a dominated route.
pub const SCREENED_ROUTE_CU: u32 = 150;
/// What discovery charges per screen on average: of a pair's two
/// directions, at most one can clear the fee screen, since a pool's buy and
/// sell prices multiply to at most one.
pub const SCREEN_CU: u32 = (LISTED_ROUTE_CU + SCREENED_ROUTE_CU) / 2;
/// Planning each candidate the executor estimates, before its preparation:
/// route bookkeeping and the budget plan.
pub const PLANNING_CU: u32 = 800;
/// Work the executor keeps, beyond the two cheapest swaps, to size and
/// execute a first candidate before discovery may enumerate routes.
pub const FIRST_CANDIDATE_CU: u32 = 12_000;
/// Fixed preparation of routes longer than two legs. Discovery, integer
/// verification and search traversal charge their own work separately.
const SIZING_CU: u32 = 1_500;
/// A two-leg solver's setup before its first iteration, and its work per
/// iteration before each venue's `interval`.
const SHORT_SIZING_CU: u32 = 1_200;
const SHORT_INTERVAL_CU: u32 = 300;
/// A longer route's work per interval, and per leg.
const LONG_INTERVAL_CU: u32 = 1_500;
const LONG_INTERVAL_LEG_CU: u32 = 400;
/// Headroom for the swap programs' cost above their reserves: the reserves
/// leave this much of the transaction's limit unused when sizing stops at
/// the budget.
pub const SAFETY_CU: u32 = 8_000;
/// The executor's own work around the swaps, and per leg beyond the invoke
/// work each venue's `swap` includes.
const EXECUTION_CU: u32 = 1_000;
const LEG_EXECUTION_CU: u32 = 100;
/// Validating the fee collector and transferring the program fee.
pub const FEE_TRANSFER_CU: u32 = 1_500;
/// The executor's execution wrapper, safety margin and fee transfer.
pub const EXECUTION_MARGIN_CU: u32 = SAFETY_CU + EXECUTION_CU + FEE_TRANSFER_CU;
/// The executor's own work per Token-2022 mint a leg transfers, beyond the swap's.
const TOKEN_2022_CU: u32 = 500;
pub const LOAN_CU: u32 = 1_400;
/// Creating a wallet token account that does not exist yet, at the
/// canonical bump: `missing_ata_cu` adds each rejected bump's derivations.
pub const MISSING_ATA_CU: u32 = missing_ata_cu(false, 255);

/// CU a transaction needs for the executor to validate and create a wallet
/// token account that does not exist yet: parsing derives its address,
/// creation derives it again in the associated-token program, and each bump
/// the derivation rejects costs both another attempt. Same-state native
/// creation costs 16,237 for a legacy account and 20,630 for Token-2022
/// with transfer-fee and metadata extensions.
pub const fn missing_ata_cu(token_2022: bool, bump: u8) -> u32 {
    let rejected = 255 - bump as u32;
    (if token_2022 { 21_000 } else { 17_000 })
        + MISSING_WALLET_PARSE_CU
        + (3_000 + 1_500) * rejected
}
pub const TRANSFER_FEE_CU: u32 = 2_700;
pub const CLMM_EXTENSION_CU: u32 = 33_000;
pub const ADAPTIVE_ORCA_CU: u32 = 4_200;
/// An adaptive-fee Whirlpool's search work per segment load, or per interval
/// of a two-leg route, beyond its venue's: the cursor's fee-group traversal.
pub const ADAPTIVE_ORCA_LOAD_CU: u32 = 1_500;
pub const PUMP_MISSING_ACCOUNT_CU: u32 = 27_700;
pub const PUMP_VOLUME_CU: u32 = 8_000;
pub const PUMP_BUY_CU: u32 = 12_000;
/// A v2 buy's extra over its sell. The costliest replayed buys exceeded the
/// costliest sells of each mint class by 3,250 to 3,430 CU.
pub const PUMP_V2_BUY_CU: u32 = 5_100;
/// A v2 swap of a Token-2022 quote mint beyond its `token_2022`: the swap
/// transfers its quote to or from the pool and again to the buyback
/// recipient.
pub const PUMP_V2_TOKEN_2022_QUOTE_CU: u32 = 4_000;
/// An Archer swap's admission and scan of each maker book it lists, beyond
/// its venue's `swap`; the executor reserves each fill past the first at the
/// venue's `crossing`.
pub const ARCHER_BOOK_CU: u32 = 2_000;
/// The executor's validation and admission of each maker book of every
/// Archer pool a transaction carries, including a deferred bid-reserve
/// projection over all its levels.
pub const ARCHER_BOOK_PARSE_CU: u32 = 6_000;
/// A swap paying our wallet makes one more token transfer: a DLMM host
/// share of the protocol fee, or a DAMM v2 referral.
pub const PAYOUT_CLAIM_CU: u32 = 3_500;
/// A funded DLMM bin holding limit orders also processes them: up to 10,331
/// CU per bin natively, beyond the crossing's 6,800.
pub const DLMM_ORDER_BIN_CU: u32 = 3_600;

/// A long route's walk charges 1,400 CU per interval to project a capped
/// transfer fee on each leg that pays one. The request's
/// headroom covered three such legs on routes without a crossing allowance,
/// the most a route converting into a legacy mint has; a fourth needs this.
const FEE_PROJECTION_CU: u32 = 1_400;
const HEADROOM_FEE_LEGS: u32 = 3;

impl VenueCost {
    /// The swap's extra work per Token-2022 mint it transfers, and ours.
    pub const fn token_2022_cu(&self) -> u32 {
        self.token_2022 + TOKEN_2022_CU
    }
}

/// Whether the executor sizes a route by walking each leg's segments, rather
/// than iterating one interval over both legs.
pub const fn is_long(legs: usize) -> bool {
    legs > 2
}

/// Entry, parsing, checks, quoting and route enumeration for every pool a
/// transaction carries.
pub fn quote_cu(groups: usize, pools: impl Iterator<Item = Venue>) -> u32 {
    let (quotes, cpmm) = pools.fold((0, 0u32), |(quotes, cpmm), venue| {
        (
            quotes + venue.cost().select,
            cpmm + u32::from(venue == Venue::RaydiumCpmm),
        )
    });
    // The fixed setup covers two CPMM descriptors. Additional descriptors
    // still parse and validate their pool, config and vault accounts.
    SETUP_CU + GROUP_SETUP_CU * groups as u32 + quotes + cpmm.saturating_sub(2) * 2_400
}

/// A route's fixed work, from its legs' venues.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteWork {
    /// Preparing the legs before the search.
    pub preparation: u32,
    /// A two-leg solver's setup before its first interval; verifying a
    /// searched input does not search.
    pub search: u32,
    /// A search interval's fixed work. A long route also loads each leg's
    /// segments, which its legs charge separately.
    pub interval: u32,
    /// Execution reserved for the swaps and our work around them, including
    /// the safety margin and the program fee transfer,
    /// before Token-2022 and account creation.
    pub execution: u32,
}

pub fn route_work(legs: &[Venue]) -> RouteWork {
    let count = legs.len() as u32;
    let long = is_long(legs.len());
    let mut work = RouteWork {
        preparation: if long { SIZING_CU } else { 0 },
        search: if long { 0 } else { SHORT_SIZING_CU },
        interval: if long {
            LONG_INTERVAL_CU + LONG_INTERVAL_LEG_CU * count
        } else {
            SHORT_INTERVAL_CU
        },
        execution: EXECUTION_MARGIN_CU + LEG_EXECUTION_CU * count,
    };
    for venue in legs {
        let cost = venue.cost();
        work.preparation += cost.prepare;
        if !long {
            work.interval += cost.interval;
        }
        work.execution += cost.swap;
    }
    work
}

/// Search and crossing allowance for a route's tick, bin and order-book legs,
/// beyond one interval: the work to walk to a large trade's size and reserve
/// its crossings, at the measured cost of a crossing. On a two-leg route the
/// DLMM allowance funds about ten bins beside a product pool; it grew by ten
/// bins' share of the crossing reserve's rise from 5,600 to 6,800, so the
/// same walk still fits.
/// Concentrated and order-book allowances fund eight to ten ticks and about
/// twenty levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Allowance {
    dlmm: u32,
    /// Raydium CLMM and its forks, including Byreal's swap-dynamic-fee pools,
    /// fixed-fee Orca Whirlpool and DLMM Pro.
    concentrated: u32,
    /// An adaptive-fee Whirlpool, whose walk also traverses fee groups.
    adaptive_orca: u32,
    /// Manifest, with or without globals, and Archer.
    order_book: u32,
    /// Each further leg that crosses ticks, bins or levels.
    extra_leg: u32,
}

/// Two-leg routes: one interval iterates both pools.
const SHORT_ALLOWANCE: Allowance = Allowance {
    dlmm: 92_000,
    concentrated: 40_000,
    adaptive_orca: 40_000,
    order_book: 60_000,
    extra_leg: 20_000,
};
/// Longer routes walk each leg's segments, which costs the search about
/// twice a two-leg interval per bin: the DLMM allowance funds the same ten
/// bins as a two-leg route's.
const LONG_ALLOWANCE: Allowance = Allowance {
    dlmm: 124_000,
    concentrated: 32_000,
    // The least swept value at which replayed three-leg routes through an
    // adaptive Whirlpool kept their unconstrained profit.
    adaptive_orca: 83_000,
    order_book: 40_000,
    extra_leg: 8_000,
};

impl Allowance {
    fn of(&self, leg: &Leg) -> Option<u32> {
        match leg.venue {
            Venue::Dlmm => Some(self.dlmm),
            Venue::Whirlpool if leg.adaptive_orca => Some(self.adaptive_orca),
            Venue::Clmm
            | Venue::ClmmFork
            | Venue::ByrealDynamicFee
            | Venue::Whirlpool
            | Venue::DlmmPro => Some(self.concentrated),
            Venue::Manifest | Venue::ManifestGlobal | Venue::Archer => Some(self.order_book),
            _ => None,
        }
    }

    /// The largest crossing leg's allowance, plus `extra_leg` for each other.
    fn for_route(&self, legs: &[Leg]) -> u32 {
        let (largest, count) = legs
            .iter()
            .filter_map(|leg| self.of(leg))
            .fold((0, 0), |(largest, count), a| (largest.max(a), count + 1));
        largest + self.extra_leg * u32::saturating_sub(count, 1)
    }
}

/// One leg of a route the executor could trade: its venue and how many of
/// the mints it transfers are Token-2022.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Leg {
    venue: Venue,
    token_2022_mints: u32,
    /// Whether this direction buys Pump's native base token. Unknown pool
    /// orientation keeps the buy reserve in either route direction.
    pump_buy: Option<bool>,
    /// Whether a Pump pool's native quote mint is Token-2022.
    pump_quote_token_2022: bool,
    /// A Whirlpool whose fee groups the executor's search walks
    /// (`walks_fee_groups`).
    adaptive_orca: bool,
}

/// Whether to fund the executor's fee-group walk through a pool: a Whirlpool
/// whose `BasketPool::adaptive_fee` is adaptive or unread. The executor walks
/// fee groups only while an adaptive schedule is active, so this can
/// over-fund.
fn walks_fee_groups(venue: Venue, adaptive_fee: Option<bool>) -> bool {
    venue == Venue::Whirlpool && adaptive_fee != Some(false)
}

/// Whether a Pump pool between mints `x` and `b` is quoted in a Token-2022
/// mint. Unknown pool orientation counts either mint.
const fn pump_quote_token_2022(quote_is_base: Option<bool>, x: bool, b: bool) -> bool {
    match quote_is_base {
        Some(true) => b,
        Some(false) => x,
        None => x || b,
    }
}

/// The client's default converter: the Raydium AMM v4 SOL/USDC pool.
pub const DEFAULT_CONVERTER: Venue = Venue::RaydiumAmm;

/// CU the executor needs to size and execute `legs` at a large trade's size,
/// beyond the transaction's setup: preparation, one search interval and the
/// crossing allowance, and the execution reserve.
fn route_cu(legs: &[Leg]) -> u32 {
    let mut venues = [Venue::RaydiumAmm; 4];
    for (venue, leg) in venues.iter_mut().zip(legs) {
        *venue = leg.venue;
    }
    let venues = &venues[..legs.len()];
    let long = is_long(legs.len());
    let loads = if long {
        venues.iter().map(|v| v.cost().load).sum::<u32>()
    } else {
        0
    };
    let transfers = legs
        .iter()
        .map(|leg| {
            leg.token_2022_mints * leg.venue.cost().token_2022_cu()
                + match leg.venue {
                    // Whether the pool pays a host fee or referral we claim
                    // is not known here.
                    Venue::Dlmm | Venue::Dammv2 | Venue::Dammv2Compounding => PAYOUT_CLAIM_CU,
                    Venue::PumpV2 if leg.pump_quote_token_2022 => PUMP_V2_TOKEN_2022_QUOTE_CU,
                    _ => 0,
                }
        })
        .sum::<u32>();
    // A route and its reverse complement every known Pump buy direction.
    // Reserve the costlier direction, rather than a buy on every Pump leg:
    // two pools with the same native orientation cannot both be buys.
    let (mut buys, mut sells, mut unknown) = (0u32, 0u32, 0u32);
    for leg in legs.iter().filter(|leg| leg.venue.is_pump()) {
        let cu = leg.venue.pump_buy_cu();
        match leg.pump_buy {
            Some(true) => buys += cu,
            Some(false) => sells += cu,
            None => unknown += cu,
        }
    }
    let pump_buys = buys.max(sells) + unknown;
    let fee_legs = legs.iter().filter(|leg| leg.token_2022_mints != 0).count() as u32;
    let fee_projections = if long {
        fee_legs.saturating_sub(HEADROOM_FEE_LEGS) * FEE_PROJECTION_CU
    } else {
        0
    };
    let adaptive_orca = legs.iter().filter(|leg| leg.adaptive_orca).count() as u32
        * (ADAPTIVE_ORCA_CU + ADAPTIVE_ORCA_LOAD_CU);
    let allowance = if long {
        LONG_ALLOWANCE
    } else {
        SHORT_ALLOWANCE
    };
    let work = route_work(venues);
    work.preparation
        + work.search
        + work.execution
        + transfers
        + pump_buys
        + fee_projections
        + adaptive_orca
        + work.interval
        + loads
        + allowance.for_route(legs)
}

/// Preparing and sizing one interval of another candidate. The executor
/// ranks most candidates by their search estimate and verifies only a trade
/// near the minimum profit, so no integer verification is added per
/// candidate. Fixed-cost routes and routes through an Archer pool are ranked
/// by their integer quote instead; an Archer quote's level walk, like
/// crossing depth, draws on the largest route's walking allowance, so a
/// further Archer candidate sizes only within what that allowance leaves.
fn candidate_cu(legs: &[Leg]) -> u32 {
    let mut venues = [Venue::RaydiumAmm; 4];
    for (venue, leg) in venues.iter_mut().zip(legs) {
        *venue = leg.venue;
    }
    let work = route_work(&venues[..legs.len()]);
    work.preparation + work.search + work.interval + PLANNING_CU + 300
}

/// A pool a transaction carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BasketPool {
    pub venue: Venue,
    /// Quoted in the settlement mint. Otherwise the route converts through
    /// the basket's converter.
    pub settlement_quoted: bool,
    /// Pump's native quote mint is this pool's route base mint. `None`
    /// preserves the buy reserve when native orientation is unavailable.
    pub pump_quote_is_base: Option<bool>,
    /// Whether a Whirlpool's state names an adaptive-fee tier: its fee-tier
    /// index differs from its tick spacing. `None`, for a pool whose state
    /// was not read, costs a Whirlpool as adaptive. Other venues ignore it.
    pub adaptive_fee: Option<bool>,
}

/// One instruction group: with bridges, its intermediate, bridges and direct
/// pools, in that order; without, its direct pools only.
#[derive(Clone, Copy, Debug)]
pub struct BasketGroup<'a> {
    pub bridges: usize,
    pub pools: &'a [BasketPool],
    pub target_token_2022: bool,
    pub base_token_2022: bool,
}

/// The pool a basket converts through, and whether the mint it converts
/// into, which the basket's crossing pools and converting bridges are quoted
/// in, is Token-2022. A venue alone converts into a legacy mint, as the
/// SOL/USDC converter does, and a Whirlpool venue alone is costed as an
/// adaptive-fee pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Converter {
    pub venue: Venue,
    pub quote_token_2022: bool,
    /// As `BasketPool::adaptive_fee`.
    pub adaptive_fee: Option<bool>,
}

impl From<Venue> for Converter {
    fn from(venue: Venue) -> Self {
        Self {
            venue,
            quote_token_2022: false,
            adaptive_fee: None,
        }
    }
}

/// CU a transaction needs for the executor to size and execute whichever
/// route its groups allow at a large trade's size. A pool quoted in the
/// conversion's mint trades through `converter`. Accounts the executor must
/// create and the maker books an Archer pool lists are not known here:
/// callers add `missing_ata_cu` for each such account, `ARCHER_BOOK_PARSE_CU`
/// for each book of every Archer pool the transaction carries, and
/// `ARCHER_BOOK_CU` for each book of the Archer pools of the route with the
/// most. The executor never exceeds its budget.
pub fn transaction_cu(
    groups: &[BasketGroup<'_>],
    converter: impl Into<Converter>,
    loan: bool,
) -> u32 {
    // The executor never exceeds its allowance, but a short one sizes the
    // trade smaller. Request a little more than the fitted need.
    let need = fitted_cu(groups, converter, loan);
    (need + need / HEADROOM_DIVISOR).min(MAX_TRANSACTION_CU)
}

/// The most `transaction_cu` requests. Every replayed basket whose fit asked
/// for more used at most 640k and found the same profit as at 1.4M.
pub const MAX_TRANSACTION_CU: u32 = 700_000;

/// One part in twenty: the 5% headroom `transaction_cu` adds to its fit.
const HEADROOM_DIVISOR: u32 = 20;

/// The fitted need `transaction_cu` adds headroom to.
fn fitted_cu(groups: &[BasketGroup<'_>], converter: impl Into<Converter>, loan: bool) -> u32 {
    let (fit, first_candidate) = fit_and_floor(groups, converter);
    fit.max(first_candidate) + if loan { LOAN_CU } else { 0 }
}

/// Other candidates a basket's request sizes besides its costliest route.
/// Keep the three costliest additional routes' sizing work. The costliest
/// route retains its walking and execution allowance; the program shares
/// the remaining search allowance among its candidates.
const SIZED_CANDIDATES: usize = 3;

/// Every route the groups allow, as its group, its pools' indexes in that
/// group and whether it trades through the converter: each pair of direct
/// pools, crossing through the converter when one is quoted in the
/// conversion's mint, and each intermediate, bridge and direct pool of a
/// stock group.
pub fn each_route_pools(groups: &[BasketGroup<'_>], mut visit: impl FnMut(usize, &[usize], bool)) {
    for (index, group) in groups.iter().enumerate() {
        let pools = group.pools;
        let directs = if group.bridges == 0 {
            0
        } else {
            (group.bridges + 1).min(pools.len())
        };
        for a in directs..pools.len() {
            for b in a + 1..pools.len() {
                // A direct cycle enters and leaves through the settlement mint.
                if !pools[a].settlement_quoted && !pools[b].settlement_quoted {
                    continue;
                }
                let converts = !(pools[a].settlement_quoted && pools[b].settlement_quoted);
                visit(index, &[a, b], converts);
            }
        }
        if group.bridges == 0 {
            continue;
        }
        for bridge in 1..directs {
            for pool in directs..pools.len() {
                visit(index, &[0, bridge, pool], !pools[bridge].settlement_quoted);
            }
        }
    }
}

/// The legs of each route `each_route_pools` lists.
fn each_route(
    groups: &[BasketGroup<'_>],
    converter: impl Into<Converter>,
    mut visit: impl FnMut(&[Leg]),
) {
    let converter = converter.into();
    let converter_leg = Leg {
        venue: converter.venue,
        token_2022_mints: u32::from(converter.quote_token_2022),
        pump_buy: None,
        pump_quote_token_2022: pump_quote_token_2022(None, converter.quote_token_2022, false),
        adaptive_orca: walks_fee_groups(converter.venue, converter.adaptive_fee),
    };
    // A settlement mint is never Token-2022; the conversion's can be.
    let quote_token_2022 =
        |pool: &BasketPool| converter.quote_token_2022 && !pool.settlement_quoted;
    each_route_pools(groups, |index, route, converts| {
        let group = &groups[index];
        let target = u32::from(group.target_token_2022);
        let base = u32::from(group.base_token_2022);
        let direct = |pool: &BasketPool, base_to_x: bool| Leg {
            venue: pool.venue,
            token_2022_mints: target + u32::from(quote_token_2022(pool)),
            pump_buy: pool.pump_quote_is_base.map(|quote| quote == base_to_x),
            pump_quote_token_2022: pump_quote_token_2022(
                pool.pump_quote_is_base,
                group.target_token_2022,
                quote_token_2022(pool),
            ),
            adaptive_orca: walks_fee_groups(pool.venue, pool.adaptive_fee),
        };
        let mut legs = [converter_leg; 4];
        let count = match *route {
            [a, b] => {
                legs[0] = direct(&group.pools[a], true);
                legs[1] = direct(&group.pools[b], false);
                2
            }
            [intermediate, bridge, pool] => {
                let (intermediate, bridge) = (&group.pools[intermediate], &group.pools[bridge]);
                legs[0] = Leg {
                    venue: intermediate.venue,
                    token_2022_mints: target + base,
                    pump_buy: intermediate.pump_quote_is_base.map(|quote| !quote),
                    pump_quote_token_2022: pump_quote_token_2022(
                        intermediate.pump_quote_is_base,
                        group.target_token_2022,
                        group.base_token_2022,
                    ),
                    adaptive_orca: walks_fee_groups(intermediate.venue, intermediate.adaptive_fee),
                };
                legs[1] = Leg {
                    venue: bridge.venue,
                    token_2022_mints: base + u32::from(quote_token_2022(bridge)),
                    pump_buy: bridge.pump_quote_is_base.map(|quote| !quote),
                    pump_quote_token_2022: pump_quote_token_2022(
                        bridge.pump_quote_is_base,
                        group.base_token_2022,
                        quote_token_2022(bridge),
                    ),
                    adaptive_orca: walks_fee_groups(bridge.venue, bridge.adaptive_fee),
                };
                legs[2] = direct(&group.pools[pool], true);
                3
            }
            _ => unreachable!("routes have two or three pools"),
        };
        visit(&legs[..count + usize::from(converts)]);
    });
}

/// The routes' fit, and the least a basket with any route needs for the
/// executor to size a first candidate at all.
fn fit_and_floor(groups: &[BasketGroup<'_>], converter: impl Into<Converter>) -> (u32, u32) {
    let converter = converter.into();
    let converts = groups
        .iter()
        .flat_map(|g| g.pools)
        .any(|p| !p.settlement_quoted);
    let setup = quote_cu(
        groups.len(),
        groups
            .iter()
            .flat_map(|g| g.pools.iter().map(|p| p.venue))
            .chain(converts.then_some(converter.venue)),
    ) + u32::from(converts && converter.quote_token_2022) * TOKEN_2022_MINT_CU
        + groups
            .iter()
            .map(|g| {
                (u32::from(g.target_token_2022) + u32::from(g.bridges > 0 && g.base_token_2022))
                    * TOKEN_2022_MINT_CU
            })
            .sum::<u32>()
        + groups
            .iter()
            .flat_map(|g| g.pools)
            .filter(|p| p.venue.is_pump())
            .count() as u32
            * PUMP_PDA_CU;
    // The costliest route, whose allowance includes one direction's sizing.
    let (mut worst, mut worst_index, mut included_sizing) = (0, 0, 0);
    let mut screens = 0u32;
    each_route(groups, converter, |legs| {
        let route = route_cu(legs);
        if route > worst {
            worst = route;
            worst_index = screens / 2;
            included_sizing = candidate_cu(legs);
        }
        screens += 2; // Both directions are screened.
    });
    // The costliest other candidates' sizing: discovery is quadratic in a
    // basket's pools, but the executor sizes only a few of the routes.
    let mut sizing = [0u32; SIZED_CANDIDATES];
    let mut index = 0;
    each_route(groups, converter, |legs| {
        let mut cu = candidate_cu(legs);
        if index == worst_index {
            cu -= included_sizing;
        }
        index += 1;
        if let Some(slot) = sizing.iter().position(|&s| cu > s) {
            sizing.copy_within(slot..SIZED_CANDIDATES - 1, slot + 1);
            sizing[slot] = cu;
        }
    });
    let fit = setup + DISCOVERY_CU.max(screens * SCREEN_CU) + worst + sizing.iter().sum::<u32>();
    // Discovery enumerates routes only while it can still afford a first
    // candidate: the two cheapest swaps, their sizing and the execution
    // margin. A plain two-pool basket can need more than its fit.
    let mut cheapest = [u32::MAX; 2];
    for venue in groups
        .iter()
        .flat_map(|g| g.pools.iter().map(|p| p.venue))
        .chain(converts.then_some(converter.venue))
    {
        let cost = venue.cost().swap;
        if cost < cheapest[0] {
            cheapest = [cost, cheapest[0]];
        } else if cost < cheapest[1] {
            cheapest[1] = cost;
        }
    }
    if screens == 0 {
        return (fit, 0);
    }
    let first_candidate = setup
        + screens * SCREEN_CU
        + cheapest
            .iter()
            .map(|c| if *c == u32::MAX { 0 } else { *c })
            .sum::<u32>()
        + FIRST_CANDIDATE_CU
        + EXECUTION_MARGIN_CU;
    (fit, first_candidate)
}

#[cfg(test)]
#[path = "cost_tests.rs"]
mod tests;
