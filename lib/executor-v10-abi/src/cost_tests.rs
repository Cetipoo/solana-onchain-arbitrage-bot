use super::*;

fn pool(venue: Venue) -> BasketPool {
    BasketPool {
        venue,
        settlement_quoted: true,
        pump_quote_is_base: None,
        adaptive_fee: Some(false),
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
    // 2.1k; execution 10.5k + 200 + swaps 48.3k and the DAMM v2 leg's
    // payout claim; the solver's 1.2k setup and one 2.8k interval.
    let (routes, _) = fit(&[pool(Venue::Dammv2), pool(Venue::RaydiumCpmm)], false);
    assert_eq!(
        routes,
        17_300 + 2_100 + 59_000 + PAYOUT_CLAIM_CU + 1_200 + 2_800
    );
    // A pool that compounds its fees reserves only a costlier swap.
    let (plain, compounding) = (Venue::Dammv2.cost(), Venue::Dammv2Compounding.cost());
    assert_eq!(
        VenueCost {
            swap: plain.swap,
            ..compounding
        },
        plain
    );
    assert_eq!(
        fit(
            &[pool(Venue::Dammv2Compounding), pool(Venue::RaydiumCpmm)],
            false
        )
        .0,
        routes + compounding.swap - plain.swap
    );
    // The executor must afford a first candidate before it enumerates:
    // setup, two screens, the two swaps, sizing and the execution margin.
    // A plain two-pool basket without a claim needs a little more than
    // its fit: setup 11.6k + discovery 6k, preparation 2.3k, execution
    // 10.7k + swaps 45.9k, the solver's setup and one interval.
    let plain = [pool(Venue::RaydiumAmm), pool(Venue::RaydiumCpmm)];
    let (routes, first_candidate) = fit(&plain, false);
    assert_eq!(routes, 17_600 + 2_300 + 56_600 + 1_200 + 2_800);
    assert_eq!(
        first_candidate,
        11_600 + 2 * SCREEN_CU + 45_900 + FIRST_CANDIDATE_CU + EXECUTION_MARGIN_CU
    );
    assert!(first_candidate > routes);
    assert_eq!(direct(&plain, false), first_candidate);
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
            token_2022: v1.token_2022,
            ..v2
        },
        v1
    );
    assert!(v2.swap < v1.swap);
    // A Token-2022 base still leaves a v2 buy below a v1 sell of legacy mints.
    assert!(v2.swap + v2.token_2022_cu() + PUMP_V2_BUY_CU < v1.swap);
    let leg = |venue, pump_buy| Leg {
        venue,
        token_2022_mints: 0,
        pump_buy: Some(pump_buy),
        pump_quote_token_2022: false,
        adaptive_orca: false,
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
fn pump_v2_reserves_a_token_2022_quote_where_the_pool_is_quoted_in_one() {
    let request = |bridges: usize, quote_is_base: Option<bool>, target: bool, stock: bool| {
        let mut pump = pool(Venue::PumpV2);
        pump.pump_quote_is_base = quote_is_base;
        // A stock group's intermediate trades the target against the
        // stock; a direct pool trades it against the settlement mint.
        let pools = [pump, pool(Venue::Dlmm), pool(Venue::Dlmm)];
        fitted_cu(
            &[BasketGroup {
                bridges,
                pools: &pools[..if bridges == 0 { 2 } else { 3 }],
                target_token_2022: target,
                base_token_2022: stock,
            }],
            DEFAULT_CONVERTER,
            false,
        )
    };
    for (bridges, target, stock, quoted_in_base) in [
        // The intermediate is quoted in a Token-2022 stock, or target.
        (1, false, true, true),
        (1, true, false, false),
        // An inverted direct pool is quoted in its Token-2022 target.
        (0, true, false, false),
    ] {
        let quoted = request(bridges, Some(quoted_in_base), target, stock);
        let other = request(bridges, Some(!quoted_in_base), target, stock);
        assert_eq!(
            quoted - other,
            PUMP_V2_TOKEN_2022_QUOTE_CU,
            "bridges={bridges}"
        );
        // An unknown orientation counts either mint.
        assert_eq!(request(bridges, None, target, stock), quoted);
    }
    // Both mints of the same program: every orientation reserves it.
    for bridges in [0, 1] {
        let legacy = request(bridges, Some(true), false, false);
        assert_eq!(request(bridges, Some(false), false, false), legacy);
        assert_eq!(request(bridges, None, false, false), legacy);
    }
    assert_eq!(
        request(1, Some(true), true, true),
        request(1, Some(false), true, true)
    );
}

#[test]
fn a_token_2022_conversion_mint_reserves_its_parse_and_transfers() {
    // A stock route whose bridge is quoted in the conversion's mint:
    // intermediate, bridge, direct and converter.
    let mut bridge = pool(Venue::RaydiumCpmm);
    bridge.settlement_quoted = false;
    let converting = [pool(Venue::RaydiumCpmm), bridge, pool(Venue::RaydiumCpmm)];
    let settled = [pool(Venue::RaydiumCpmm); 3];
    let need_via =
        |venue: Venue, pools: &[BasketPool], target_token_2022: bool, quote_token_2022: bool| {
            fitted_cu(
                &[BasketGroup {
                    bridges: 1,
                    pools,
                    target_token_2022,
                    base_token_2022: false,
                }],
                Converter {
                    venue,
                    quote_token_2022,
                    adaptive_fee: Some(false),
                },
                false,
            )
        };
    let need_with = |pools: &[BasketPool], target_token_2022: bool, quote_token_2022: bool| {
        need_via(Venue::Dlmm, pools, target_token_2022, quote_token_2022)
    };
    let need =
        |pools: &[BasketPool], quote_token_2022: bool| need_with(pools, false, quote_token_2022);
    // The mint's extensions, and its transfer in the bridge and the
    // converter legs.
    assert_eq!(
        need(&converting, true) - need(&converting, false),
        TOKEN_2022_MINT_CU
            + Venue::RaydiumCpmm.cost().token_2022_cu()
            + Venue::Dlmm.cost().token_2022_cu()
    );
    // A venue alone converts into a legacy mint, and a basket that
    // does not convert reserves nothing for the conversion's mint.
    assert_eq!(
        need(&converting, false),
        fitted_cu(
            &[BasketGroup {
                bridges: 1,
                pools: &converting,
                target_token_2022: false,
                base_token_2022: false,
            }],
            Venue::Dlmm,
            false
        )
    );
    assert_eq!(need(&settled, true), need(&settled, false));
    // Beside a Token-2022 target, the conversion's mint makes all four
    // legs transfer one: the walk's fee projection on the fourth.
    assert_eq!(
        need_with(&converting, true, true) - need_with(&converting, true, false),
        need(&converting, true) - need(&converting, false) + FEE_PROJECTION_CU
    );
    // A Pump v2 converter of unknown orientation may be quoted in that
    // mint, which the executor charges on any leg.
    assert_eq!(
        need_via(Venue::PumpV2, &converting, false, true)
            - need_via(Venue::PumpV2, &converting, false, false),
        TOKEN_2022_MINT_CU
            + Venue::RaydiumCpmm.cost().token_2022_cu()
            + Venue::PumpV2.cost().token_2022_cu()
            + PUMP_V2_TOKEN_2022_QUOTE_CU
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
    // A swap-dynamic-fee pool costs a plain fork's work plus surcharges at
    // least a quarter over the measured 2,796 executor CU in selection and
    // 31,580 Byreal CU in the swap.
    let (fork, dynamic) = (Venue::ClmmFork.cost(), Venue::ByrealDynamicFee.cost());
    assert_eq!(
        VenueCost {
            select: fork.select,
            swap: fork.swap,
            ..dynamic
        },
        fork
    );
    assert!(4 * (dynamic.select - fork.select) >= 5 * 2_796);
    assert!(4 * (dynamic.swap - fork.swap) >= 5 * 31_580);
}

#[test]
fn each_crossing_venue_draws_its_walks_allowance() {
    let allowance = Allowance {
        dlmm: 1,
        concentrated: 2,
        adaptive_orca: 3,
        order_book: 4,
        extra_leg: 0,
    };
    let of = |venue, adaptive_orca| {
        allowance.of(&Leg {
            venue,
            token_2022_mints: 0,
            pump_buy: None,
            pump_quote_token_2022: false,
            adaptive_orca,
        })
    };
    let cases = [
        (Venue::Dlmm, false, Some(1)),
        (Venue::Clmm, false, Some(2)),
        (Venue::ClmmFork, false, Some(2)),
        (Venue::ByrealDynamicFee, false, Some(2)),
        (Venue::Whirlpool, false, Some(2)),
        (Venue::DlmmPro, false, Some(2)),
        (Venue::Whirlpool, true, Some(3)),
        (Venue::Manifest, false, Some(4)),
        (Venue::ManifestGlobal, false, Some(4)),
        (Venue::Archer, false, Some(4)),
        (Venue::RaydiumAmm, false, None),
        (Venue::MeteoraDamm, false, None),
        (Venue::Pump, false, None),
        (Venue::PumpV2, false, None),
        (Venue::RaydiumCpmm, false, None),
        (Venue::Dammv2, false, None),
        (Venue::Dammv2Compounding, false, None),
        (Venue::Futarchy, false, None),
        (Venue::HyloUsdc, false, None),
        (Venue::HyloStability, false, None),
        (Venue::HyloLst, false, None),
        (Venue::HyloExo, false, None),
        (Venue::HyloLstLever, false, None),
        (Venue::HyloExoLever, false, None),
        (Venue::HyloLstConvert, false, None),
        (Venue::HyloExoConvert, false, None),
        (Venue::StakePool, false, None),
    ];
    for (venue, adaptive_orca, expected) in cases {
        assert_eq!(of(venue, adaptive_orca), expected, "{venue:?}");
    }
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
        pump_quote_token_2022: false,
        adaptive_orca: false,
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
        pump_quote_token_2022: false,
        adaptive_orca: false,
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

#[test]
fn an_adaptive_whirlpool_funds_its_fee_group_walk() {
    let leg = |venue, adaptive_orca| Leg {
        venue,
        token_2022_mints: 0,
        pump_buy: None,
        pump_quote_token_2022: false,
        adaptive_orca,
    };
    let extra = |others: &[Venue]| {
        let route = |adaptive| {
            let mut legs = [leg(Venue::RaydiumAmm, false); 4];
            legs[0] = leg(Venue::Whirlpool, adaptive);
            for (slot, &venue) in legs[1..].iter_mut().zip(others) {
                *slot = leg(venue, false);
            }
            route_cu(&legs[..1 + others.len()])
        };
        route(true) - route(false)
    };
    // Its 4,200 execution reserve and 1,500 per load; a two-leg route's
    // walking allowance is the concentrated 40,000.
    assert_eq!(extra(&[Venue::RaydiumCpmm]), 4_200 + 1_500);
    // A longer route's allowance rises from 32,000 to 83,000 when the
    // Whirlpool is its largest crossing leg, and not beside a DLMM leg.
    assert_eq!(
        extra(&[Venue::RaydiumCpmm, Venue::RaydiumAmm]),
        4_200 + 1_500 + 51_000
    );
    assert_eq!(extra(&[Venue::Dlmm, Venue::RaydiumAmm]), 4_200 + 1_500);
    // A Whirlpool without its state is costed as adaptive.
    assert!(walks_fee_groups(Venue::Whirlpool, None));
    assert!(walks_fee_groups(Venue::Whirlpool, Some(true)));
    assert!(!walks_fee_groups(Venue::Whirlpool, Some(false)));
    assert!(!walks_fee_groups(Venue::Clmm, Some(true)));
    assert!(!walks_fee_groups(DEFAULT_CONVERTER, None));
    assert_eq!(Converter::from(Venue::Whirlpool).adaptive_fee, None);
}
