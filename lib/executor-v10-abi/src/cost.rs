//! The executor's CU cost model: per-venue costs measured on SVM replays of
//! captured trades, and the fixed work around them. The program budgets its
//! search with these, and clients size a transaction's limit with the same
//! numbers.
//!
//! Every search charge is the work the native integer search measurably
//! performs for that operation, with a margin of about a quarter over its
//! median; execution reserves cover the swap programs' own cost near their
//! 99th percentile, the executor's invoke work per leg, and a safety margin.

/// A pool venue the executor can trade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Venue {
    Manifest,
    RaydiumAmm,
    MeteoraDamm,
    Pump,
    RaydiumCpmm,
    Clmm,
    /// PancakeSwap and Byreal, Raydium CLMM forks with costlier swaps.
    ClmmFork,
    Whirlpool,
    Dlmm,
    Dammv2,
}

/// One venue's work. `select` parses, validates and quotes a pool before the
/// search starts, and enumerates its routes. Two-leg solvers charge
/// `interval` per combined iteration; longer routes charge `load` for every
/// segment and `search_crossing` for every one past the first, and `bound`
/// for each pass of the walk's stop check. `swap` reserves the swap program's
/// execution and the executor's invoke work around it; `crossing` each tick,
/// bin or level the swap crosses; `token_2022` is the swap's extra cost per
/// Token-2022 mint it transfers.
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
            Self::Clmm => VenueCost {
                select: 4_300,
                prepare: 5_000,
                interval: 3_000,
                load: 1_500,
                search_crossing: 3_000,
                bound: 300,
                swap: 46_500,
                crossing: 10_000,
                token_2022: 8_000,
            },
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
                // 6,703.
                // A typical bin's 4-6k would let long walks overrun.
                crossing: 6_800,
                token_2022: 5_600,
            },
            Self::Dammv2 => VenueCost {
                select: 3_000,
                prepare: 800,
                interval: 1_300,
                load: 500,
                search_crossing: 0,
                bound: 150,
                swap: 26_500,
                crossing: 0,
                token_2022: 6_000,
            },
        }
    }
}

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
pub const PUMP_MISSING_ACCOUNT_CU: u32 = 27_700;
pub const PUMP_VOLUME_CU: u32 = 8_000;
pub const PUMP_BUY_CU: u32 = 12_000;
/// A DLMM swap paying our wallet its host share of the protocol fee makes
/// one more token transfer.
pub const DLMM_HOST_CLAIM_CU: u32 = 3_500;
/// A funded DLMM bin holding limit orders also processes them: up to 10,331
/// CU per bin natively, beyond the crossing's 6,800.
pub const DLMM_ORDER_BIN_CU: u32 = 3_600;

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
/// DLMM allowance funds about ten bins beside a product pool, which keeps
/// the captured trades' profit within a tenth of a percent of the previous
/// requests; it grew by ten bins' share of the
/// crossing reserve's rise from 5,600 to 6,800, so the same walk still fits.
/// Concentrated and order-book allowances fund eight to ten ticks and about
/// twenty levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Allowance {
    dlmm: u32,
    /// Raydium CLMM and its forks, and Orca Whirlpool.
    concentrated: u32,
    order_book: u32,
    /// Each further leg that crosses ticks, bins or levels.
    extra_leg: u32,
}

/// Two-leg routes: one interval iterates both pools.
const SHORT_ALLOWANCE: Allowance = Allowance {
    dlmm: 92_000,
    concentrated: 40_000,
    order_book: 60_000,
    extra_leg: 20_000,
};
/// Longer routes walk each leg's segments.
const LONG_ALLOWANCE: Allowance = Allowance {
    dlmm: 92_000,
    concentrated: 32_000,
    order_book: 40_000,
    extra_leg: 8_000,
};

impl Allowance {
    fn of(&self, venue: Venue) -> Option<u32> {
        match venue {
            Venue::Dlmm => Some(self.dlmm),
            Venue::Clmm | Venue::ClmmFork | Venue::Whirlpool => Some(self.concentrated),
            Venue::Manifest => Some(self.order_book),
            _ => None,
        }
    }

    /// The largest crossing leg's allowance, plus `extra_leg` for each other.
    fn for_route(&self, legs: &[Venue]) -> u32 {
        let (largest, count) = legs
            .iter()
            .filter_map(|&venue| self.of(venue))
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
                    // Whether the pool pays a host fee we claim is not known here.
                    Venue::Dlmm => DLMM_HOST_CLAIM_CU,
                    _ => 0,
                }
        })
        .sum::<u32>();
    // A route and its reverse complement every known Pump buy direction.
    // Reserve the costlier direction, rather than a buy on every Pump leg:
    // two pools with the same native orientation cannot both be buys.
    let (mut buys, mut sells, mut unknown) = (0u32, 0u32, 0u32);
    for leg in legs.iter().filter(|leg| leg.venue == Venue::Pump) {
        match leg.pump_buy {
            Some(true) => buys += 1,
            Some(false) => sells += 1,
            None => unknown += 1,
        }
    }
    let pump_buys = PUMP_BUY_CU * (buys.max(sells) + unknown);
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
        + work.interval
        + loads
        + allowance.for_route(venues)
}

/// Preparing and sizing one interval of another candidate. The executor
/// ranks candidates by their search estimate and verifies only a trade near
/// the minimum profit, so no integer verification is added per candidate.
/// Crossing depth remains covered by the largest route's walking allowance.
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
    /// the Raydium AMM SOL/USDC pool.
    pub settlement_quoted: bool,
    /// Pump's native quote mint is this pool's route base mint. `None`
    /// preserves the buy reserve when native orientation is unavailable.
    pub pump_quote_is_base: Option<bool>,
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

/// CU a transaction needs for the executor to size and execute whichever
/// route its groups allow at a large trade's size. A pool quoted in the
/// other settlement mint trades through a SOL/USDC pool of venue
/// `converter`. Accounts the executor must create are not known here;
/// callers add `missing_ata_cu` for each. The executor never exceeds its
/// budget.
pub fn transaction_cu(groups: &[BasketGroup<'_>], converter: Venue, loan: bool) -> u32 {
    // The executor never exceeds its allowance, but a short one sizes the
    // trade smaller. Request a little more than the fitted need.
    let need = fitted_cu(groups, converter, loan);
    (need + need / HEADROOM_DIVISOR).min(MAX_TRANSACTION_CU)
}

/// The most `transaction_cu` requests. Every captured basket whose fit asked
/// for more used at most 640k and found the same profit as at 1.4M.
pub const MAX_TRANSACTION_CU: u32 = 700_000;

/// One part in twenty: the 5% headroom `transaction_cu` adds to its fit.
const HEADROOM_DIVISOR: u32 = 20;

/// The fitted need `transaction_cu` adds headroom to.
fn fitted_cu(groups: &[BasketGroup<'_>], converter: Venue, loan: bool) -> u32 {
    let (fit, first_candidate) = fit_and_floor(groups, converter);
    fit.max(first_candidate) + if loan { LOAN_CU } else { 0 }
}

/// Other candidates a basket's request sizes besides its costliest route.
/// Keep the three costliest additional routes' sizing work. The costliest
/// route retains its walking and execution allowance; the program shares
/// the remaining search allowance among its candidates.
const SIZED_CANDIDATES: usize = 3;

/// Every route the groups allow: each pair of direct pools, crossing through
/// the converter when one is quoted in the other settlement mint, and each
/// intermediate, bridge and direct pool of a stock group.
fn each_route(groups: &[BasketGroup<'_>], converter: Venue, mut visit: impl FnMut(&[Leg])) {
    let converter_leg = Leg {
        venue: converter,
        token_2022_mints: 0,
        pump_buy: None,
    };
    for group in groups {
        let target = u32::from(group.target_token_2022);
        let direct = |pool: &BasketPool, base_to_x: bool| Leg {
            venue: pool.venue,
            token_2022_mints: target,
            pump_buy: pool.pump_quote_is_base.map(|quote| quote == base_to_x),
        };
        let directs = if group.bridges == 0 {
            group.pools
        } else {
            &group.pools[(group.bridges + 1).min(group.pools.len())..]
        };
        for (i, a) in directs.iter().enumerate() {
            for b in &directs[i + 1..] {
                // A direct cycle enters and leaves through the settlement mint.
                if !a.settlement_quoted && !b.settlement_quoted {
                    continue;
                }
                let converts = !(a.settlement_quoted && b.settlement_quoted);
                let legs = [direct(a, true), direct(b, false), converter_leg];
                visit(&legs[..2 + usize::from(converts)]);
            }
        }
        if group.bridges == 0 {
            continue;
        }
        let base = u32::from(group.base_token_2022);
        let Some((intermediate, rest)) = group.pools.split_first() else {
            continue;
        };
        let (bridges, directs) = rest.split_at(group.bridges.min(rest.len()));
        for bridge in bridges {
            for pool in directs {
                let legs = [
                    Leg {
                        venue: intermediate.venue,
                        token_2022_mints: target + base,
                        pump_buy: intermediate.pump_quote_is_base.map(|quote| !quote),
                    },
                    Leg {
                        venue: bridge.venue,
                        token_2022_mints: base,
                        pump_buy: bridge.pump_quote_is_base.map(|quote| !quote),
                    },
                    direct(pool, true),
                    converter_leg,
                ];
                let converts = !bridge.settlement_quoted;
                visit(&legs[..3 + usize::from(converts)]);
            }
        }
    }
}

/// The routes' fit, and the least a basket with any route needs for the
/// executor to size a first candidate at all.
fn fit_and_floor(groups: &[BasketGroup<'_>], converter: Venue) -> (u32, u32) {
    let converts = groups
        .iter()
        .flat_map(|g| g.pools)
        .any(|p| !p.settlement_quoted);
    let setup = quote_cu(
        groups.len(),
        groups
            .iter()
            .flat_map(|g| g.pools.iter().map(|p| p.venue))
            .chain(converts.then_some(converter)),
    ) + groups
        .iter()
        .map(|g| {
            (u32::from(g.target_token_2022) + u32::from(g.bridges > 0 && g.base_token_2022))
                * TOKEN_2022_MINT_CU
        })
        .sum::<u32>()
        + groups
            .iter()
            .flat_map(|g| g.pools)
            .filter(|p| p.venue == Venue::Pump)
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
        .chain(converts.then_some(converter))
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
mod tests {
    use super::*;

    fn pool(venue: Venue) -> BasketPool {
        BasketPool {
            venue,
            settlement_quoted: true,
            pump_quote_is_base: None,
        }
    }

    fn direct(pools: &[BasketPool], token_2022: bool) -> u32 {
        fitted_cu(
            &[BasketGroup {
                bridges: 0,
                pools,
                target_token_2022: token_2022,
                base_token_2022: false,
            }],
            DEFAULT_CONVERTER,
            false,
        )
    }

    fn fit(pools: &[BasketPool], token_2022: bool) -> (u32, u32) {
        fit_and_floor(
            &[BasketGroup {
                bridges: 0,
                pools,
                target_token_2022: token_2022,
                base_token_2022: false,
            }],
            DEFAULT_CONVERTER,
        )
    }

    #[test]
    fn constant_product_pairs_need_no_crossing_allowance() {
        // Setup 3.5k + 1.5k + quotes 3k + 3.3k + discovery 6k; preparation
        // 2.1k; execution 10.5k + 200 + swaps 53.8k; the solver's 1.2k setup
        // and one 2.8k interval.
        let (routes, first_candidate) =
            fit(&[pool(Venue::Dammv2), pool(Venue::RaydiumCpmm)], false);
        assert_eq!(routes, 17_300 + 2_100 + 64_500 + 1_200 + 2_800);
        // The executor must afford a first candidate before it enumerates:
        // setup, two screens, the two swaps, sizing and the execution margin.
        // A plain two-pool basket needs a little more than its fit.
        assert_eq!(
            first_candidate,
            11_300 + 2 * SCREEN_CU + 53_800 + FIRST_CANDIDATE_CU + EXECUTION_MARGIN_CU
        );
        assert!(first_candidate > routes);
        assert_eq!(
            direct(&[pool(Venue::Dammv2), pool(Venue::RaydiumCpmm)], false),
            first_candidate
        );
        // Pump's quote, derivation, preparation, swap and buy, and its
        // Token-2022 transfer.
        let cpmm = fit(&[pool(Venue::Dammv2), pool(Venue::RaydiumCpmm)], true).0;
        let pump = fit(&[pool(Venue::Dammv2), pool(Venue::Pump)], true).0;
        let (c, p) = (Venue::RaydiumCpmm.cost(), Venue::Pump.cost());
        assert_eq!(
            pump + c.select + c.prepare + c.interval + c.swap + c.token_2022,
            cpmm + p.select
                + PUMP_PDA_CU
                + p.prepare
                + p.interval
                + p.swap
                + PUMP_BUY_CU
                + p.token_2022
        );
    }

    #[test]
    fn pump_buy_reserve_covers_every_orientation_and_cycle_direction() {
        // Pool order is direct buy/sell, or intermediate/bridge/direct as
        // Route::via constructs them. Reversing a cycle reverses every leg.
        for (bridges, forward) in [(0, &[true, false][..]), (1, &[false, false, true][..])] {
            for converts in [false, true] {
                for converter in [Venue::RaydiumAmm, Venue::Pump] {
                    let mut storage = [pool(Venue::Pump); 3];
                    let pools = &mut storage[..forward.len()];
                    pools[1].settlement_quoted = !converts;
                    let route_cost = |pools: &[BasketPool]| {
                        let mut cost = None;
                        each_route(
                            &[BasketGroup {
                                bridges,
                                pools,
                                target_token_2022: false,
                                base_token_2022: false,
                            }],
                            converter,
                            |legs| {
                                assert!(cost.is_none());
                                cost = Some(route_cu(legs));
                            },
                        );
                        cost.unwrap()
                    };
                    let unknown_cost = route_cost(pools);
                    let unknown_converter = usize::from(converts && converter == Venue::Pump);
                    // Each pool is unknown, native quote = x, or native quote
                    // = route base. Exhaust all compatible native states.
                    for mut metadata in 0..3usize.pow(pools.len() as u32) {
                        for pool in pools.iter_mut() {
                            pool.pump_quote_is_base = match metadata % 3 {
                                0 => None,
                                1 => Some(false),
                                _ => Some(true),
                            };
                            metadata /= 3;
                        }
                        let mut max_buys = 0;
                        for native in 0..(1 << pools.len()) {
                            if pools.iter().enumerate().any(|(i, pool)| {
                                pool.pump_quote_is_base
                                    .is_some_and(|quote| quote != (native & (1 << i) != 0))
                            }) {
                                continue;
                            }
                            for reverse in [false, true] {
                                let buys = forward
                                    .iter()
                                    .enumerate()
                                    .filter(|(i, direction)| {
                                        (**direction ^ reverse) == (native & (1 << i) != 0)
                                    })
                                    .count()
                                    + unknown_converter;
                                max_buys = max_buys.max(buys);
                            }
                        }
                        let saved_buys = pools.len() + unknown_converter - max_buys;
                        assert_eq!(
                            unknown_cost - route_cost(pools),
                            saved_buys as u32 * PUMP_BUY_CU,
                            "bridges={bridges}, converts={converts}, converter={converter:?}, pools={pools:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_execution_margins_cover_the_safety_and_wrapper_work() {
        assert_eq!(EXECUTION_MARGIN_CU, 10_500);
        assert_eq!(SCREEN_CU, 825);
        // Execution reserves keep the measured swap costs and crossings.
        let dlmm = Venue::Dlmm.cost();
        assert_eq!(
            (dlmm.swap, dlmm.crossing, dlmm.token_2022),
            (35_400, 6_800, 5_600)
        );
        assert_eq!(Venue::ClmmFork.cost().swap, 59_500);
    }

    #[test]
    fn a_basket_sizes_only_its_costliest_other_candidates() {
        // Every DLMM pair costs the same. Four pools already have more than
        // three other candidates, so a fifth adds only its quote and the
        // screens of its four new pairs.
        let pools = [pool(Venue::Dlmm); 6];
        let fit = |n: usize| fit(&pools[..n], false).0;
        // Five pools screen twenty routes, above discovery's 10k floor.
        assert_eq!(fit(6) - fit(5), Venue::Dlmm.cost().select + 10 * SCREEN_CU);
    }

    #[test]
    fn long_preparation_preserves_search_and_execution_reserves() {
        let legs = [Venue::Dlmm, Venue::Clmm, Venue::Pump, Venue::RaydiumAmm];
        // Native planning charges 1.5k fixed preparation plus each venue's
        // preparation. Enumeration and verification have separate charges.
        let three = route_work(&legs[..3]);
        assert_eq!(three.preparation, 1_500 + 2_300 + 5_000 + 300);
        assert_eq!(three.search, 0);
        assert_eq!(three.interval, 1_500 + 3 * 400);
        assert_eq!(
            three.execution,
            EXECUTION_MARGIN_CU + 300 + 35_400 + 46_500 + 82_700
        );

        let four = route_work(&legs);
        assert_eq!(four.preparation, three.preparation + 1_000);
        assert_eq!(four.search, 0);
        assert_eq!(four.interval, three.interval + 400);
        assert_eq!(four.execution, three.execution + 18_600 + 100);
    }

    #[test]
    fn converted_wide_baskets_use_native_long_preparation() {
        // These baskets contain both two-leg and converted three-leg routes.
        // The costliest route keeps its full walking/execution reserve.
        // Three further routes share the reduced preparation allowance.
        for (venue, count, converted) in [
            (Venue::Dlmm, 5, 3),
            (Venue::Dlmm, 7, 4),
            (Venue::Whirlpool, 5, 3),
        ] {
            let mut pools = [pool(venue); 7];
            for pool in &mut pools[..converted] {
                pool.settlement_quoted = false;
            }
            let groups = [BasketGroup {
                bridges: 0,
                pools: &pools[..count],
                target_token_2022: false,
                base_token_2022: false,
            }];
            let request = transaction_cu(&groups, DEFAULT_CONVERTER, false);
            // The converted three-leg route is the costliest; the request
            // stays below the cap and above the two-leg request.
            let two_leg = transaction_cu(
                &[BasketGroup {
                    bridges: 0,
                    pools: &[pool(venue), pool(venue)],
                    target_token_2022: false,
                    base_token_2022: false,
                }],
                DEFAULT_CONVERTER,
                false,
            );
            assert!(request > two_leg && request < MAX_TRANSACTION_CU);
        }
    }

    #[test]
    fn the_widest_basket_stays_below_the_cap() {
        let pools = [pool(Venue::Clmm); 16];
        let groups = [BasketGroup {
            bridges: 0,
            pools: &pools,
            target_token_2022: true,
            base_token_2022: false,
        }];
        let request = transaction_cu(&groups, DEFAULT_CONVERTER, true);
        assert!(request < MAX_TRANSACTION_CU);
        // 240 screens, the costliest pair's walk and three more candidates.
        assert!(request > 400_000);
    }

    #[test]
    fn the_transaction_limit_adds_headroom_to_the_fit() {
        let pools = [pool(Venue::Dlmm), pool(Venue::Clmm)];
        let groups = [BasketGroup {
            bridges: 0,
            pools: &pools,
            target_token_2022: false,
            base_token_2022: false,
        }];
        let need = fitted_cu(&groups, DEFAULT_CONVERTER, true);
        assert_eq!(
            transaction_cu(&groups, DEFAULT_CONVERTER, true),
            need + need / 20
        );
    }

    #[test]
    fn the_costliest_route_and_every_quote_set_the_limit() {
        let dlmm_pump = direct(&[pool(Venue::Dlmm), pool(Venue::Pump)], false);
        // A third pool is quoted and its additional pairs are sized too.
        let with_cpmm = direct(
            &[
                pool(Venue::Dlmm),
                pool(Venue::Pump),
                pool(Venue::RaydiumCpmm),
            ],
            false,
        );
        let leg = |venue| Leg {
            venue,
            token_2022_mints: 0,
            pump_buy: None,
        };
        // Its quote, and discovery stays at its 10k floor for six screens.
        assert_eq!(
            with_cpmm,
            dlmm_pump
                + Venue::RaydiumCpmm.cost().select
                + candidate_cu(&[leg(Venue::Dlmm), leg(Venue::RaydiumCpmm)])
                + candidate_cu(&[leg(Venue::Pump), leg(Venue::RaydiumCpmm)])
        );
        // Two crossing legs add the extra-leg allowance.
        let two = direct(&[pool(Venue::Dlmm), pool(Venue::Clmm)], false);
        let one = direct(&[pool(Venue::Dlmm), pool(Venue::RaydiumCpmm)], false);
        assert!(two > one + SHORT_ALLOWANCE.extra_leg);
    }

    #[test]
    fn conversions_and_bridges_route_through_longer_cycles() {
        let quoted = direct(&[pool(Venue::Dlmm), pool(Venue::Pump)], false);
        let mut other = pool(Venue::Dlmm);
        other.settlement_quoted = false;
        let converting = direct(&[other, pool(Venue::Pump)], false);
        // The converter is quoted and swapped, and the cycle becomes long.
        assert!(converting > quoted);
        // Two pools in the other mint alone form no cycle.
        let mut both = [other, other];
        both[1].venue = Venue::Clmm;
        assert_eq!(
            direct(&both, false),
            quote_cu(1, [Venue::Dlmm, Venue::Clmm, Venue::RaydiumAmm].into_iter()) + DISCOVERY_CU
        );
        let stock = |bridge: BasketPool| {
            fitted_cu(
                &[BasketGroup {
                    bridges: 1,
                    pools: &[pool(Venue::Dlmm), bridge, pool(Venue::Pump)],
                    target_token_2022: false,
                    base_token_2022: false,
                }],
                DEFAULT_CONVERTER,
                true,
            )
        };
        let mut converting_bridge = pool(Venue::Clmm);
        converting_bridge.settlement_quoted = false;
        assert!(stock(converting_bridge) > stock(pool(Venue::Clmm)));
        let legs = [Venue::Dlmm, Venue::Clmm, Venue::Pump].map(|venue| Leg {
            venue,
            token_2022_mints: 0,
            pump_buy: None,
        });
        // The only route's sizing is included in its own allowance.
        assert_eq!(
            stock(pool(Venue::Clmm)) - LOAN_CU,
            quote_cu(1, [Venue::Dlmm, Venue::Clmm, Venue::Pump].into_iter())
                + PUMP_PDA_CU
                + DISCOVERY_CU
                + route_cu(&legs)
        );
    }
}
