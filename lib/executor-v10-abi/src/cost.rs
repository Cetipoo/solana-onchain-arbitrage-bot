//! The executor's CU cost model: per-venue costs fitted to SVM runs, and the
//! fixed work around them. The program budgets its search with these, and
//! clients size a transaction's limit with the same numbers.

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

/// One venue's work. `select` quotes a pool before the search starts.
/// Two-leg solvers charge `interval` per combined iteration; longer routes
/// charge `load` for every segment and `search_crossing` for every one past
/// the first, and `bound` for each pass of the walk's stop check. `swap` and
/// `crossing` reserve execution; `token_2022` is the swap's extra cost per
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
                select: 3_400,
                prepare: 800,
                interval: 2_000,
                load: 1_200,
                search_crossing: 2_000,
                bound: 600,
                swap: 15_000,
                crossing: 2_000,
                token_2022: 3_000,
            },
            Self::RaydiumAmm => VenueCost {
                select: 5_200,
                prepare: 1_000,
                interval: 2_000,
                load: 1_200,
                search_crossing: 0,
                bound: 600,
                swap: 16_500,
                crossing: 0,
                token_2022: 0,
            },
            Self::MeteoraDamm => VenueCost {
                select: 6_500,
                prepare: 800,
                interval: 2_000,
                load: 1_200,
                search_crossing: 0,
                bound: 600,
                swap: 78_500,
                crossing: 0,
                token_2022: 3_000,
            },
            Self::Pump => VenueCost {
                select: 6_600,
                prepare: 800,
                interval: 2_000,
                load: 1_200,
                search_crossing: 0,
                bound: 600,
                swap: 80_000,
                crossing: 0,
                token_2022: 3_000,
            },
            Self::RaydiumCpmm => VenueCost {
                select: 1_500,
                prepare: 600,
                interval: 2_000,
                load: 1_200,
                search_crossing: 0,
                bound: 600,
                swap: 25_500,
                crossing: 0,
                token_2022: 4_100,
            },
            Self::Clmm => VenueCost {
                select: 5_800,
                prepare: 1_300,
                interval: 10_000,
                load: 1_300,
                search_crossing: 9_600,
                bound: 700,
                swap: 43_500,
                crossing: 10_000,
                token_2022: 8_000,
            },
            Self::ClmmFork => VenueCost {
                swap: 56_500,
                ..Self::Clmm.cost()
            },
            Self::Whirlpool => VenueCost {
                select: 7_300,
                prepare: 1_400,
                interval: 10_000,
                load: 1_300,
                search_crossing: 9_600,
                bound: 700,
                swap: 38_500,
                crossing: 7_100,
                token_2022: 9_900,
            },
            Self::Dlmm => VenueCost {
                select: 7_000,
                prepare: 1_000,
                interval: 3_000,
                load: 500,
                search_crossing: 11_300,
                bound: 2_700,
                swap: 33_000,
                crossing: 5_600,
                token_2022: 5_600,
            },
            Self::Dammv2 => VenueCost {
                select: 5_000,
                prepare: 800,
                interval: 2_000,
                load: 1_200,
                search_crossing: 0,
                bound: 600,
                swap: 15_000,
                crossing: 0,
                token_2022: 3_100,
            },
        }
    }
}

/// Parsing, checks and the brand log before any pool is quoted, and per
/// instruction group.
const SETUP_CU: u32 = 5_500;
const GROUP_SETUP_CU: u32 = 3_600;
/// Quoting work beyond each pool's `select`, such as bin and tick scans,
/// which the program measures as it goes.
const DISCOVERY_CU: u32 = 10_000;
/// Fixed sizing work of routes longer than two legs.
const SIZING_CU: u32 = 13_700;
/// A two-leg solver's work per iteration, before each venue's `interval`.
const SHORT_INTERVAL_CU: u32 = 5_000;
/// A longer route's work per interval, and per leg.
const LONG_INTERVAL_CU: u32 = 1_000;
const LONG_INTERVAL_LEG_CU: u32 = 500;
/// Headroom for estimate error: the fitted costs leave this much of the
/// transaction's limit unused when sizing stops at the budget.
const SAFETY_CU: u32 = 15_000;
/// The executor's own work around the swaps, and per leg.
const EXECUTION_CU: u32 = 3_800;
const LEG_EXECUTION_CU: u32 = 2_600;
/// The executor's own work per Token-2022 mint a leg transfers, beyond the swap's.
const TOKEN_2022_CU: u32 = 1_000;
pub const LOAN_CU: u32 = 1_400;
pub const MISSING_ATA_CU: u32 = 22_000;
pub const TRANSFER_FEE_CU: u32 = 2_700;
pub const CLMM_EXTENSION_CU: u32 = 33_000;
pub const ADAPTIVE_ORCA_CU: u32 = 4_200;
pub const PUMP_MISSING_ACCOUNT_CU: u32 = 27_700;
pub const PUMP_VOLUME_CU: u32 = 8_000;
pub const PUMP_BUY_CU: u32 = 10_100;

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

/// Parsing, checks, and quoting every pool a transaction carries.
pub fn quote_cu(groups: usize, pools: impl Iterator<Item = Venue>) -> u32 {
    SETUP_CU + GROUP_SETUP_CU * groups as u32 + pools.map(|v| v.cost().select).sum::<u32>()
}

/// A route's fixed work, from its legs' venues.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteWork {
    /// Preparing the legs before the search.
    pub preparation: u32,
    /// A search interval's fixed work. A long route also loads each leg's
    /// segments, which its legs charge separately.
    pub interval: u32,
    /// Execution reserved for the swaps and our work around them, including
    /// the safety margin, before Token-2022 and account creation.
    pub execution: u32,
}

pub fn route_work(legs: &[Venue]) -> RouteWork {
    let count = legs.len() as u32;
    let long = is_long(legs.len());
    let mut work = RouteWork {
        preparation: if long { SIZING_CU } else { 0 },
        interval: if long {
            LONG_INTERVAL_CU + LONG_INTERVAL_LEG_CU * count
        } else {
            SHORT_INTERVAL_CU
        },
        execution: SAFETY_CU + EXECUTION_CU + LEG_EXECUTION_CU * count,
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
/// its crossings, so large trades keep their full profit.
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
    dlmm: 90_000,
    concentrated: 60_000,
    order_book: 20_000,
    extra_leg: 30_000,
};
/// Longer routes walk each leg's segments.
const LONG_ALLOWANCE: Allowance = Allowance {
    dlmm: 140_000,
    concentrated: 40_000,
    order_book: 40_000,
    extra_leg: 10_000,
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
}

/// The Raydium AMM SOL/USDC pool every conversion goes through.
const CONVERTER: Leg = Leg {
    venue: Venue::RaydiumAmm,
    token_2022_mints: 0,
};

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
                + if leg.venue == Venue::Pump {
                    PUMP_BUY_CU
                } else {
                    0
                }
        })
        .sum::<u32>();
    let allowance = if long {
        LONG_ALLOWANCE
    } else {
        SHORT_ALLOWANCE
    };
    let work = route_work(venues);
    work.preparation
        + work.execution
        + transfers
        + work.interval
        + loads
        + allowance.for_route(venues)
}

/// A pool a transaction carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BasketPool {
    pub venue: Venue,
    /// Quoted in the settlement mint. Otherwise the route converts through
    /// the Raydium AMM SOL/USDC pool.
    pub settlement_quoted: bool,
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
/// route its groups allow at a large trade's size. Accounts the executor
/// must create are not known here, so a first trade on a new mint sizes
/// slightly smaller; the executor never exceeds its budget.
pub fn transaction_cu(groups: &[BasketGroup<'_>], loan: bool) -> u32 {
    let converts = groups
        .iter()
        .flat_map(|g| g.pools)
        .any(|p| !p.settlement_quoted);
    let setup = quote_cu(
        groups.len(),
        groups
            .iter()
            .flat_map(|g| g.pools.iter().map(|p| p.venue))
            .chain(converts.then_some(Venue::RaydiumAmm)),
    ) + DISCOVERY_CU;
    let mut worst = 0;
    for group in groups {
        let target = u32::from(group.target_token_2022);
        let direct = |pool: &BasketPool| Leg {
            venue: pool.venue,
            token_2022_mints: target,
        };
        if group.bridges == 0 {
            for (i, a) in group.pools.iter().enumerate() {
                for b in &group.pools[i + 1..] {
                    // A direct cycle enters and leaves through the settlement mint.
                    if !a.settlement_quoted && !b.settlement_quoted {
                        continue;
                    }
                    let converts = !(a.settlement_quoted && b.settlement_quoted);
                    let legs = [direct(a), direct(b), CONVERTER];
                    worst = worst.max(route_cu(&legs[..2 + usize::from(converts)]));
                }
            }
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
                    },
                    Leg {
                        venue: bridge.venue,
                        token_2022_mints: base,
                    },
                    direct(pool),
                    CONVERTER,
                ];
                let converts = !bridge.settlement_quoted;
                worst = worst.max(route_cu(&legs[..3 + usize::from(converts)]));
            }
        }
    }
    setup + worst + if loan { LOAN_CU } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(venue: Venue) -> BasketPool {
        BasketPool {
            venue,
            settlement_quoted: true,
        }
    }

    fn direct(pools: &[BasketPool], token_2022: bool) -> u32 {
        transaction_cu(
            &[BasketGroup {
                bridges: 0,
                pools,
                target_token_2022: token_2022,
                base_token_2022: false,
            }],
            false,
        )
    }

    #[test]
    fn constant_product_pairs_need_no_crossing_allowance() {
        // Setup 5.5k + 3.6k + quotes 5k + 1.5k + discovery 10k; preparation
        // 1.4k; execution 15k + 3.8k + 5.2k + swaps 40.5k; one interval 9k.
        assert_eq!(
            direct(&[pool(Venue::Dammv2), pool(Venue::RaydiumCpmm)], false),
            25_600 + 1_400 + 64_500 + 9_000
        );
        // Pump's quote, preparation, swap and buy, and its Token-2022 transfer.
        let cpmm = direct(&[pool(Venue::Dammv2), pool(Venue::RaydiumCpmm)], true);
        let pump = direct(&[pool(Venue::Dammv2), pool(Venue::Pump)], true);
        assert_eq!(
            pump + 1_500 + 600 + 25_500 + 4_100,
            cpmm + 6_600 + 800 + 80_000 + PUMP_BUY_CU + 3_000
        );
    }

    #[test]
    fn the_costliest_route_and_every_quote_set_the_limit() {
        let dlmm_pump = direct(&[pool(Venue::Dlmm), pool(Venue::Pump)], false);
        // A third pool is quoted, and the costliest pair it allows decides.
        let with_cpmm = direct(
            &[
                pool(Venue::Dlmm),
                pool(Venue::Pump),
                pool(Venue::RaydiumCpmm),
            ],
            false,
        );
        assert_eq!(with_cpmm, dlmm_pump + 1_500);
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
            transaction_cu(
                &[BasketGroup {
                    bridges: 1,
                    pools: &[pool(Venue::Dlmm), bridge, pool(Venue::Pump)],
                    target_token_2022: false,
                    base_token_2022: false,
                }],
                true,
            )
        };
        let mut converting_bridge = pool(Venue::Clmm);
        converting_bridge.settlement_quoted = false;
        assert!(stock(converting_bridge) > stock(pool(Venue::Clmm)));
        assert_eq!(
            stock(pool(Venue::Clmm)) - LOAN_CU,
            quote_cu(1, [Venue::Dlmm, Venue::Clmm, Venue::Pump].into_iter())
                + DISCOVERY_CU
                + route_cu(&[
                    Leg {
                        venue: Venue::Dlmm,
                        token_2022_mints: 0
                    },
                    Leg {
                        venue: Venue::Clmm,
                        token_2022_mints: 0
                    },
                    Leg {
                        venue: Venue::Pump,
                        token_2022_mints: 0
                    },
                ])
        );
    }
}
