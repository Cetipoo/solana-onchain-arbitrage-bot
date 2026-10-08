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
    let (routes, first_candidate) = fit(&[pool(Venue::Dammv2), pool(Venue::RaydiumCpmm)], false);
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
fn pump_v2_reserves_a_cheaper_swap_and_buy_beside_v1_legs() {
    let (v1, v2) = (Venue::Pump.cost(), Venue::PumpV2.cost());
    assert_eq!(
        VenueCost {
            swap: v1.swap,
            ..v2
        },
        v1
    );
    assert!(v2.swap < v1.swap);
    let leg = |venue, pump_buy| Leg {
        venue,
        token_2022_mints: 0,
        pump_buy: Some(pump_buy),
    };
    let route = |a, b| route_cu(&[a, b]);
    // A selling leg buys when the route runs in reverse. The reserve is
    // the costlier direction's buys: here the v1 buy either way.
    let opposite = route(leg(Venue::Pump, true), leg(Venue::PumpV2, false));
    assert_eq!(
        opposite,
        route(leg(Venue::Pump, false), leg(Venue::PumpV2, true))
    );
    // Both buy in one direction.
    let together = route(leg(Venue::Pump, true), leg(Venue::PumpV2, true));
    assert_eq!(together - opposite, PUMP_V2_BUY_CU);
    assert_eq!(
        together,
        route(leg(Venue::Pump, false), leg(Venue::PumpV2, false))
    );
    // An unknown direction reserves its buy on top of the costlier one.
    let unknown = route_cu(&[
        leg(Venue::Pump, true),
        Leg {
            pump_buy: None,
            ..leg(Venue::PumpV2, true)
        },
    ]);
    assert_eq!(unknown, together);
    assert_eq!(
        route(leg(Venue::Pump, true), leg(Venue::Pump, false))
            - route(leg(Venue::PumpV2, true), leg(Venue::PumpV2, false)),
        2 * (v1.swap - v2.swap) + PUMP_BUY_CU - PUMP_V2_BUY_CU
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
