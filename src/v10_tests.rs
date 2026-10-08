use super::*;

fn request() -> V10Instruction {
    let wallet = Pubkey::new_unique();
    V10Instruction {
        program_id: Pubkey::new_unique(),
        wallet,
        settlement: MintAccounts::ata(&wallet, SOL, TOKEN).unwrap(),
        conversion: None,
        fee_collector: Pubkey::new_unique(),
        additional_fee_collector: None,
        flashloan: None,
        header: Header {
            compute_unit_limit: 400_000,
            ..Header::default()
        },
        groups: vec![],
    }
}

fn pool(x: MintAccounts, base: MintAccounts) -> PoolAccounts {
    let mut state = vec![0; 328];
    for (offset, key) in [
        (168, x.mint),
        (200, base.mint),
        (72, Pubkey::new_unique()),
        (104, Pubkey::new_unique()),
        (8, Pubkey::new_unique()),
        (296, Pubkey::new_unique()),
    ] {
        state[offset..offset + 32].copy_from_slice(key.as_ref());
    }
    PoolAccounts::from_state(Pubkey::new_unique(), CPMM, &state, x, base, &[], None).unwrap()
}

fn group(request: &V10Instruction) -> DirectGroup {
    let target = MintAccounts::ata(&request.wallet, Pubkey::new_unique(), TOKEN).unwrap();
    DirectGroup {
        target,
        pools: vec![
            pool(target, request.settlement),
            pool(target, request.settlement),
        ],
    }
}

#[test]
fn direct_group_counts_and_account_layout_round_trip() {
    let request = request();
    let groups: Vec<_> = (0..MAX_GROUPS).map(|_| group(&request)).collect();
    let ix = request.build_direct(&groups).unwrap();
    assert_eq!(ix.data[0], OPCODE);
    let args = InstructionData::decode(&ix.data[1..]).unwrap();
    assert_eq!(args.group_count as usize, MAX_GROUPS);
    assert_eq!(args.account_count(), ix.accounts.len());
    assert!(args
        .groups
        .iter()
        .all(|g| g.is_direct() && g.pool_count() == 2));
}

#[test]
fn direct_builder_rejects_invalid_counts_and_mint_roles() {
    let request = request();
    let good = group(&request);
    assert!(request.build_direct(&[]).is_err());
    assert!(request
        .build_direct(&vec![good.clone(); MAX_GROUPS + 1])
        .is_err());
    for count in [0, 1, MAX_POOLS + 1] {
        let mut bad = good.clone();
        bad.pools = vec![good.pools[0].clone(); count];
        assert!(request.build_direct(&[bad]).is_err());
    }
    let mut bad = good.clone();
    bad.target = request.settlement;
    assert!(request.build_direct(&[bad]).is_err());
    let other = MintAccounts::ata(&request.wallet, Pubkey::new_unique(), TOKEN).unwrap();
    for invalid in [pool(other, request.settlement), pool(good.target, other)] {
        let mut bad = good.clone();
        bad.pools[1] = invalid;
        assert!(request.build_direct(&[bad]).is_err());
    }
}

#[test]
fn direct_builder_rejects_triangle_state_and_repeated_conversion() {
    let mut request = request();
    let mut direct = group(&request);
    let base = MintAccounts::ata(&request.wallet, Pubkey::new_unique(), TOKEN).unwrap();
    request.groups.push(MarketGroup {
        target: direct.target,
        base,
        intermediate: pool(direct.target, base),
        bridges: vec![pool(base, request.settlement)],
        direct: direct.pools.clone(),
    });
    assert!(request.build_direct(&[direct.clone()]).is_err());
    request.groups.clear();
    let quote = MintAccounts::ata(&request.wallet, USDC, TOKEN).unwrap();
    let mut state = vec![0; 752];
    for (offset, key) in [
        (400, quote.mint),
        (432, request.settlement.mint),
        (336, Pubkey::new_unique()),
        (368, Pubkey::new_unique()),
    ] {
        state[offset..offset + 32].copy_from_slice(key.as_ref());
    }
    request.conversion = Some(ConversionAccounts {
        quote,
        pool: PoolAccounts::from_state(
            Pubkey::new_unique(),
            RAYDIUM,
            &state,
            quote,
            request.settlement,
            &[],
            None,
        )
        .unwrap(),
    });
    direct.pools[1] = pool(direct.target, quote);
    let ix = request.build_direct(&[direct.clone()]).unwrap();
    assert_eq!(
        InstructionData::decode(&ix.data[1..])
            .unwrap()
            .account_count(),
        ix.accounts.len()
    );
    direct.pools[0] = pool(direct.target, quote);
    assert!(request.build_direct(&[direct]).is_err());
}

fn raydium_state(x: MintAccounts, base: MintAccounts) -> Vec<u8> {
    let mut state = vec![0; 752];
    for (offset, key) in [
        (400, x.mint),
        (432, base.mint),
        (336, Pubkey::new_unique()),
        (368, Pubkey::new_unique()),
    ] {
        state[offset..offset + 32].copy_from_slice(key.as_ref());
    }
    state
}

fn meteora_state(a: Pubkey, b: Pubkey) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut pool = vec![0; 952];
    let mut a_vault = vec![0; 1232];
    let mut b_vault = vec![0; 1232];
    for (offset, key) in [
        (40, a),
        (72, b),
        (104, Pubkey::new_unique()),
        (136, Pubkey::new_unique()),
        (168, Pubkey::new_unique()),
        (200, Pubkey::new_unique()),
        (234, Pubkey::new_unique()),
        (266, Pubkey::new_unique()),
    ] {
        pool[offset..offset + 32].copy_from_slice(key.as_ref());
    }
    pool[233] = 1;
    for (vault, mint) in [(&mut a_vault, a), (&mut b_vault, b)] {
        for (offset, key) in [
            (19, Pubkey::new_unique()),
            (83, mint),
            (115, Pubkey::new_unique()),
        ] {
            vault[offset..offset + 32].copy_from_slice(key.as_ref());
        }
    }
    (pool, a_vault, b_vault)
}

#[test]
fn raydium_and_meteora_are_accepted_as_intermediates_and_bridges() {
    let mut request = request();
    let target = MintAccounts::ata(&request.wallet, Pubkey::new_unique(), TOKEN).unwrap();
    let base = MintAccounts::ata(&request.wallet, Pubkey::new_unique(), TOKEN).unwrap();
    let raydium = |x: MintAccounts, b: MintAccounts| {
        PoolAccounts::from_state(
            Pubkey::new_unique(),
            RAYDIUM,
            &raydium_state(x, b),
            x,
            b,
            &[],
            None,
        )
        .unwrap()
    };
    let meteora = |x: MintAccounts, b: MintAccounts| {
        // Token A is the base here, so the builder must flip the block orientation.
        let (pool, a_vault, b_vault) = meteora_state(b.mint, x.mint);
        PoolAccounts::from_meteora_state(Pubkey::new_unique(), &pool, &a_vault, &b_vault, x, b)
            .unwrap()
    };
    for (intermediate, bridge) in [
        (raydium(target, base), meteora(base, request.settlement)),
        (meteora(target, base), raydium(base, request.settlement)),
    ] {
        request.groups = vec![MarketGroup {
            target,
            base,
            intermediate,
            bridges: vec![bridge],
            direct: vec![
                pool(target, request.settlement),
                meteora(target, request.settlement),
            ],
        }];
        let ix = request.build().unwrap();
        let args = InstructionData::decode(&ix.data[1..]).unwrap();
        assert_eq!(args.account_count(), ix.accounts.len());
        assert_eq!(
            args.groups[0].pool_account_counts[..4]
                .iter()
                .filter(|&&n| n == 14)
                .count(),
            2
        );
    }
}

#[test]
fn meteora_block_orients_vault_accounts_by_declared_pair() {
    let request = request();
    let x = MintAccounts::ata(&request.wallet, Pubkey::new_unique(), TOKEN).unwrap();
    let (pool, a_vault, b_vault) = meteora_state(request.settlement.mint, x.mint);
    let block = PoolAccounts::from_meteora_state(
        Pubkey::new_unique(),
        &pool,
        &a_vault,
        &b_vault,
        x,
        request.settlement,
    )
    .unwrap();
    let key = |d: &[u8], offset: usize| {
        Pubkey::new_from_array(d[offset..offset + 32].try_into().unwrap())
    };
    let accounts = block.accounts();
    assert_eq!(accounts.len(), 14);
    assert_eq!(accounts[0].pubkey, METEORA);
    assert_eq!(accounts[1].pubkey, request.settlement.mint);
    assert_eq!(accounts[2].pubkey, METEORA_VAULT);
    assert_eq!(accounts[4].pubkey, key(&pool, 136));
    assert_eq!(accounts[5].pubkey, key(&pool, 104));
    assert_eq!(accounts[6].pubkey, key(&b_vault, 19));
    assert_eq!(accounts[7].pubkey, key(&a_vault, 19));
    assert_eq!(accounts[8].pubkey, key(&b_vault, 115));
    assert_eq!(accounts[9].pubkey, key(&a_vault, 115));
    assert_eq!(accounts[10].pubkey, key(&pool, 200));
    assert_eq!(accounts[11].pubkey, key(&pool, 168));
    assert_eq!(accounts[12].pubkey, key(&pool, 266));
    assert_eq!(accounts[13].pubkey, key(&pool, 234));
    assert!(accounts[3..].iter().all(|a| a.is_writable));
    assert!(PoolAccounts::from_state(
        Pubkey::new_unique(),
        METEORA,
        &pool,
        x,
        request.settlement,
        &[],
        None
    )
    .is_err());
    let token_2022 = MintAccounts::ata(&request.wallet, x.mint, TOKEN_2022).unwrap();
    assert!(PoolAccounts::from_meteora_state(
        Pubkey::new_unique(),
        &pool,
        &a_vault,
        &b_vault,
        token_2022,
        request.settlement
    )
    .is_err());
    let mut disabled = pool.clone();
    disabled[233] = 0;
    assert!(PoolAccounts::from_meteora_state(
        Pubkey::new_unique(),
        &disabled,
        &a_vault,
        &b_vault,
        x,
        request.settlement
    )
    .is_err());
    let mut stable = pool.clone();
    stable[874] = 1;
    assert!(PoolAccounts::from_meteora_state(
        Pubkey::new_unique(),
        &stable,
        &a_vault,
        &b_vault,
        x,
        request.settlement
    )
    .is_err());
    assert!(PoolAccounts::from_meteora_state(
        Pubkey::new_unique(),
        &pool,
        &b_vault,
        &a_vault,
        x,
        request.settlement
    )
    .is_err());
    assert!(array_candidates(Pubkey::new_unique(), METEORA, &pool, 2)
        .unwrap()
        .0
        .is_empty());
}

#[test]
fn manifest_market_validation_rejects_wrong_version_and_account_kind() {
    let mut state = vec![0; 256];
    state[..8].copy_from_slice(&4859840929024028656u64.to_le_bytes());
    assert!(PoolPair::read(MANIFEST, &state).is_ok());
    state[8] = 1;
    assert!(PoolPair::read(MANIFEST, &state).is_err());
    state[8] = 0;
    assert!(PoolPair::read(MANIFEST, &state[..255]).is_err());
    state[0] ^= 1;
    assert!(PoolPair::read(MANIFEST, &state).is_err());
}

#[test]
fn manifest_markets_need_a_spare_order_node() {
    let wallet = Pubkey::new_unique();
    let x = MintAccounts::ata(&wallet, Pubkey::new_unique(), TOKEN).unwrap();
    let base = MintAccounts::ata(&wallet, SOL, TOKEN).unwrap();
    let mut state = vec![0; 256 + 3 * 80];
    state[..8].copy_from_slice(&4859840929024028656u64.to_le_bytes());
    for (offset, key) in [
        (16, x.mint),
        (48, base.mint),
        (80, Pubkey::new_unique()),
        (112, Pubkey::new_unique()),
    ] {
        state[offset..offset + 32].copy_from_slice(key.as_ref());
    }
    let load = |free: u32, next: u32| {
        let mut state = state.clone();
        state[176..180].copy_from_slice(&free.to_le_bytes());
        if let Some(node) = state.get_mut(256 + free as usize..260 + free as usize) {
            node.copy_from_slice(&next.to_le_bytes());
        }
        PoolAccounts::from_state(Pubkey::new_unique(), MANIFEST, &state, x, base, &[], None)
    };
    assert!(load(80, 160).is_ok());
    // A self-loop, a lone free node, an empty free list, a misaligned or
    // out-of-bounds link: the executor would reject each market.
    for (free, next) in [
        (80, 80),
        (80, u32::MAX),
        (u32::MAX, 80),
        (80, 170),
        (80, 400),
    ] {
        let error = load(free, next).unwrap_err().to_string();
        assert!(error.contains("no spare order node"), "{error}");
    }
}

fn pump(
    wallet: Pubkey,
    target: MintAccounts,
    quote: MintAccounts,
    cashback: bool,
    v2: bool,
) -> Result<PoolAccounts> {
    PoolAccounts::from_pump_keys(
        Pubkey::new_unique(),
        wallet,
        target,
        quote,
        PumpKeys {
            vaults: [Pubkey::new_unique(), Pubkey::new_unique()],
            mint0: target.mint,
            quote,
            recipient: Pubkey::new_unique(),
            creator: Pubkey::new_unique(),
            cashback,
            buyback: Pubkey::new_unique(),
            v2,
        },
    )
}

#[test]
fn compact_pump_blocks_list_only_what_v2_needs() {
    let request = request();
    let wallet = request.wallet;
    let target = MintAccounts::ata(&wallet, Pubkey::new_unique(), TOKEN).unwrap();
    let direct = |v2| {
        let pools = vec![
            pump(wallet, target, request.settlement, false, v2).unwrap(),
            pump(wallet, target, request.settlement, false, v2).unwrap(),
        ];
        request
            .build_direct(&[DirectGroup { target, pools }])
            .unwrap()
    };
    let compact = direct(true);
    let args = InstructionData::decode(&compact.data[1..]).unwrap();
    assert_eq!(args.account_count(), compact.accounts.len());
    assert!(args.groups[0].pool_account_counts[..2]
        .iter()
        .all(|&n| usize::from(n) == PUMP_V2_POOL_ACCOUNTS));
    // Each compact block lists ten of the legacy block's eighteen accounts.
    // The transaction holds each key once. Compact blocks drop each pool's
    // fee recipient, creator vault, their quote accounts and the buyback
    // owner, plus the target's pool-v2 PDA, the global volume accumulator
    // and the fee program the two pools share.
    let legacy = direct(false);
    assert_eq!(legacy.accounts.len() - compact.accounts.len(), 2 * 8);
    let unique = |ix: &Instruction| {
        ix.accounts
            .iter()
            .map(|a| a.pubkey)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    };
    assert_eq!(unique(&legacy) - unique(&compact), 2 * 5 + 3);

    let pool = pump(wallet, target, request.settlement, false, true).unwrap();
    assert_eq!(pool.venue(), Venue::PumpV2);
    assert_eq!(pool.pump_quote_is_base(), Some(true));
    let legacy = pump(wallet, target, request.settlement, false, false).unwrap();
    assert_eq!(legacy.venue(), Venue::Pump);
    assert_eq!(legacy.accounts()[..2], pool.accounts()[..2]);

    // V2 never trades cashback coins or other quotes.
    assert!(pump(wallet, target, request.settlement, true, true).is_err());
    let other = MintAccounts::ata(&wallet, Pubkey::new_unique(), TOKEN).unwrap();
    assert!(pump(wallet, target, other, false, true).is_err());
    let sol_2022 = MintAccounts::ata(&wallet, SOL, TOKEN_2022).unwrap();
    assert!(pump(wallet, target, sol_2022, false, true).is_err());
}
