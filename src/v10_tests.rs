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

/// A Raydium CPMM pool's state trading `a` against `b`, with unique vaults,
/// config and observation.
pub(crate) fn cpmm_state([a, b]: [Pubkey; 2]) -> Vec<u8> {
    let mut state = vec![0; 328];
    for (offset, key) in [
        (168, a),
        (200, b),
        (72, Pubkey::new_unique()),
        (104, Pubkey::new_unique()),
        (8, Pubkey::new_unique()),
        (296, Pubkey::new_unique()),
    ] {
        state[offset..offset + 32].copy_from_slice(key.as_ref());
    }
    state
}

fn pool(x: MintAccounts, base: MintAccounts) -> PoolAccounts {
    let state = cpmm_state([x.mint, base.mint]);
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
    let account = solana_sdk::account::Account {
        owner: METEORA,
        data: pool,
        ..Default::default()
    };
    assert_eq!(
        crate::arrays::array_walks(Pubkey::new_unique(), &account, |_| None).unwrap(),
        [vec![], vec![]]
    );
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

#[test]
fn dlmm_pro_block_follows_the_executors_layout() {
    let request = request();
    let x = MintAccounts::ata(&request.wallet, Pubkey::new_unique(), TOKEN_2022).unwrap();
    let base = request.settlement;
    // A mainnet pool, whose oracle successful swaps list beside the
    // program's pool authority and event authority.
    let pool = pubkey!("2BDqjWftjXUDUKQnbtJG8bQSHkY1mXhgvoWSmJ5HEDQe");
    let (reserve_x, reserve_y) = (Pubkey::new_unique(), Pubkey::new_unique());
    let mut state = vec![0; 648];
    state[..8].copy_from_slice(&dlmm_pro::POOL_DISCRIMINATOR);
    state[564..566].copy_from_slice(&1u16.to_le_bytes());
    for (offset, key) in [
        (8, x.mint),
        (40, base.mint),
        (72, reserve_x),
        (104, reserve_y),
    ] {
        state[offset..offset + 32].copy_from_slice(key.as_ref());
    }
    let arrays = [Pubkey::new_unique(), Pubkey::new_unique()];
    let build = |state: &[u8], x: MintAccounts, base: MintAccounts, arrays: &[Pubkey]| {
        PoolAccounts::from_state(pool, DLMM_PRO, state, x, base, arrays, None)
    };
    let block = build(&state, x, base, &arrays).unwrap();
    let accounts = block.accounts();
    let expected = [
        (DLMM_PRO, false),
        (base.mint, false),
        (pool, true),
        (reserve_x, true),
        (reserve_y, true),
        (
            pubkey!("DSUCk2WhfKVtGEhcrCtXvJQsPsSNtTkX5JmsWz5uSahL"),
            true,
        ),
        (
            pubkey!("FSUXyUKbTJ4A5Ccz5M6Uo4J2myc3RsDKTPU9vZwyeXru"),
            false,
        ),
        (
            pubkey!("6D8NF9HL6eTrheURXz9fLvrL1L7Uzfy5JAA2bCAUEN13"),
            false,
        ),
        (MEMO, false),
        (arrays[0], true),
        (arrays[1], true),
    ];
    assert_eq!(
        accounts
            .iter()
            .map(|meta| (meta.pubkey, meta.is_writable))
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(block.venue(), Venue::DlmmPro);
    // The declared x side may be the pool's y mint.
    let flipped = build(&state, base, x, &arrays).unwrap();
    assert_eq!(flipped.accounts()[1].pubkey, x.mint);
    assert_eq!(flipped.accounts()[3].pubkey, reserve_y);
    assert_eq!(flipped.accounts()[4].pubkey, reserve_x);

    let error = |state: &[u8], arrays: &[Pubkey], bitmap: Option<Pubkey>| {
        PoolAccounts::from_state(pool, DLMM_PRO, state, x, base, arrays, bitmap)
            .unwrap_err()
            .to_string()
    };
    let array_count = "V10 requires 1..=6 arrays per DLMM Pro pool";
    assert_eq!(error(&state, &[], None), array_count);
    assert_eq!(error(&state, &[Pubkey::new_unique(); 7], None), array_count);
    assert!(build(&state, x, base, &[Pubkey::new_unique(); 6]).is_ok());
    let flag_mismatch = "DLMM Pro bitmap extension does not match its pool flag";
    assert_eq!(error(&state, &arrays, Some(pool)), flag_mismatch);
    let mut extension = state.clone();
    extension[576] = 1;
    assert_eq!(error(&extension, &arrays, None), flag_mismatch);
    let bitmap = dlmm_pro::bitmap_extension(&pool);
    let extended =
        PoolAccounts::from_state(pool, DLMM_PRO, &extension, x, base, &arrays, Some(bitmap))
            .unwrap();
    assert_eq!(extended.accounts()[9], AccountMeta::new(bitmap, false));
    assert_eq!(&extended.accounts()[10..], &accounts[9..]);
    let mut unknown = state.clone();
    unknown[575] = 4;
    assert_eq!(
        error(&unknown, &arrays, None),
        format!("DLMM Pro pool {pool} has unsupported extensions 0x04")
    );
    let mut flat = state.clone();
    flat[564..566].fill(0);
    assert_eq!(
        error(&flat, &arrays, None),
        format!("DLMM Pro pool {pool} has a tick spacing of 0")
    );
    let mut other = state.clone();
    other[0] ^= 1;
    assert_eq!(
        error(&other, &arrays, None),
        format!("{pool} is not a DLMM Pro pool")
    );
}

fn byreal_state(x: MintAccounts, base: MintAccounts, feeds: [[u8; 32]; 2]) -> Vec<u8> {
    let mut state = vec![0; 1544];
    state[73..105].copy_from_slice(x.mint.as_ref());
    state[105..137].copy_from_slice(base.mint.as_ref());
    state[137..169].copy_from_slice(Pubkey::new_unique().as_ref());
    state[169..201].copy_from_slice(Pubkey::new_unique().as_ref());
    state[1112..1144].copy_from_slice(&feeds[0]);
    state[1144..1176].copy_from_slice(&feeds[1]);
    state
}

#[test]
fn byreal_swap_dynamic_fee_pools_list_memo_and_their_price_updates() {
    // Pyth's USDC/USD and USDT/USD feed ids and the push oracle's shard-0
    // price update accounts for them on mainnet.
    let feeds = [
        "eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a",
        "2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b",
    ]
    .map(|feed| std::array::from_fn(|i| u8::from_str_radix(&feed[2 * i..2 * i + 2], 16).unwrap()));
    let updates = [
        pubkey!("Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX"),
        pubkey!("HT2PLQBcG5EiCcNSaMHAjSgd9F98ecpATbk4Sk5oYuM"),
    ];
    let wallet = Pubkey::new_unique();
    let x = MintAccounts::ata(&wallet, Pubkey::new_unique(), TOKEN).unwrap();
    let base = MintAccounts::ata(&wallet, Pubkey::new_unique(), TOKEN).unwrap();
    let arrays = [Pubkey::new_unique()];
    let mut state = byreal_state(x, base, feeds);
    let load = |owner, state: &[u8]| {
        PoolAccounts::from_state(
            Pubkey::new_unique(),
            owner,
            state,
            x,
            base,
            &arrays,
            Some(Pubkey::new_unique()),
        )
        .unwrap()
    };
    let lists_memo = |pool: &PoolAccounts| pool.accounts().iter().any(|meta| meta.pubkey == MEMO);

    let fixed = load(BYREAL, &state);
    assert!(!lists_memo(&fixed));
    assert_eq!(fixed.accounts().last().unwrap().pubkey, arrays[0]);
    assert_eq!(fixed.venue(), Venue::ClmmFork);

    state[1096] = 1 << 4;
    assert_eq!(
        byreal_price_updates(&state).unwrap(),
        Some([0, 1].map(|token| PythPriceUpdate {
            account: updates[token],
            feed: feeds[token],
        }))
    );
    let dynamic = load(BYREAL, &state);
    assert!(lists_memo(&dynamic));
    assert_eq!(dynamic.venue(), Venue::ByrealDynamicFee);
    let accounts = dynamic.accounts();
    assert_eq!(accounts.len(), fixed.accounts().len() + 4);
    let tail = &accounts[accounts.len() - 4..];
    assert_eq!(tail[0], AccountMeta::new(arrays[0], false));
    assert_eq!(tail[1], AccountMeta::new_readonly(TOKEN_2022, false));
    for (meta, update) in tail[2..].iter().zip(updates) {
        assert_eq!(*meta, AccountMeta::new_readonly(update, false));
    }
    // Only Byreal's layout gives byte 1096 that meaning.
    let pancake = load(PANCAKESWAP, &state);
    assert!(!lists_memo(&pancake));
    assert_eq!(pancake.venue(), Venue::ClmmFork);
    let error = PoolAccounts::from_keys(
        Pubkey::new_unique(),
        x,
        base,
        [Pubkey::new_unique(), Pubkey::new_unique()],
        PoolKeys::Clmm {
            program: PANCAKESWAP,
            config: Pubkey::new_unique(),
            observation: Pubkey::new_unique(),
            bitmap: Pubkey::new_unique(),
            arrays: &arrays,
            pyth: Some(updates),
        },
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "only Byreal pools take Pyth price updates"
    );
}

/// A `PriceUpdateV2` of `feed` whose verification level tag is `level`,
/// with the price message at byte 42 after a `Partial` level (tag 0 and
/// a signature count) or 41 after `Full` (tag 1), `len` bytes long.
pub(crate) fn price_update(level: u8, feed: [u8; 32], len: usize) -> Vec<u8> {
    let mut data = vec![0; len];
    data[..8].copy_from_slice(&[0x22, 0xf1, 0x23, 0x63, 0x9d, 0x7e, 0xf4, 0xcd]);
    data[40] = level;
    let message = if level == 0 { 42 } else { 41 };
    data[message..message + 32].copy_from_slice(&feed);
    data
}

#[test]
fn a_price_update_names_its_feed_after_its_verification_level() {
    let feed = [7; 32];
    assert_eq!(
        price_update_feed(&price_update(0, feed, 134)).unwrap(),
        feed
    );
    assert_eq!(
        price_update_feed(&price_update(1, feed, 133)).unwrap(),
        feed
    );
    let error = |data: &[u8]| price_update_feed(data).unwrap_err().to_string();
    let short = "Pyth price update data too short";
    assert_eq!(error(&price_update(0, feed, 133)), short);
    assert_eq!(error(&price_update(1, feed, 132)), short);
    assert_eq!(
        error(&price_update(2, feed, 134)),
        "unknown Pyth verification level"
    );
    let mut other = price_update(1, feed, 133);
    other[0] ^= 1;
    assert_eq!(error(&other), "invalid Pyth price update discriminator");
}

#[test]
fn concentrated_blocks_list_the_pool_before_its_accounts_and_arrays() {
    let wallet = Pubkey::new_unique();
    let legacy = MintAccounts::ata(&wallet, Pubkey::new_unique(), TOKEN).unwrap();
    let token_2022 = MintAccounts::ata(&wallet, Pubkey::new_unique(), TOKEN_2022).unwrap();
    let base = MintAccounts::ata(&wallet, Pubkey::new_unique(), TOKEN).unwrap();
    let [pool, x_vault, base_vault, config, observation, bitmap, oracle] =
        std::array::from_fn(|_| Pubkey::new_unique());
    let arrays = [Pubkey::new_unique(), Pubkey::new_unique()];
    let updates = [Pubkey::new_unique(), Pubkey::new_unique()];
    let clmm = |program, pyth| PoolKeys::Clmm {
        program,
        config,
        observation,
        bitmap,
        arrays: &arrays,
        pyth,
    };
    let dlmm = |bitmap| PoolKeys::Dlmm {
        oracle,
        bitmap,
        arrays: &arrays,
    };
    let whirlpool_oracle = Pubkey::find_program_address(&[b"oracle", pool.as_ref()], &WHIRLPOOL).0;
    let dlmm_event_authority = Pubkey::find_program_address(&[b"__event_authority"], &DLMM).0;
    let (w, r) = (true, false);
    let cases = [
        (
            "Raydium CLMM of legacy mints",
            legacy,
            clmm(CLMM, None),
            vec![
                (CLMM, r),
                (base.mint, r),
                (pool, w),
                (config, r),
                (observation, w),
                (bitmap, w),
                (x_vault, w),
                (base_vault, w),
                (arrays[0], w),
                (arrays[1], w),
            ],
        ),
        (
            "Byreal swap-dynamic-fee",
            legacy,
            clmm(BYREAL, Some(updates)),
            vec![
                (BYREAL, r),
                (base.mint, r),
                (MEMO, r),
                (pool, w),
                (config, r),
                (observation, w),
                (bitmap, w),
                (x_vault, w),
                (base_vault, w),
                (arrays[0], w),
                (arrays[1], w),
                (TOKEN_2022, r),
                (updates[0], r),
                (updates[1], r),
            ],
        ),
        (
            "Whirlpool",
            legacy,
            PoolKeys::Whirlpool {
                arrays: &arrays,
                adaptive_fee: Some(false),
            },
            vec![
                (WHIRLPOOL, r),
                (base.mint, r),
                (MEMO, r),
                (pool, w),
                (whirlpool_oracle, w),
                (x_vault, w),
                (base_vault, w),
                (arrays[0], w),
                (arrays[1], w),
            ],
        ),
        (
            "DLMM of a Token-2022 mint with a bitmap extension",
            token_2022,
            dlmm(Some(bitmap)),
            vec![
                (DLMM, r),
                (base.mint, r),
                (dlmm_event_authority, r),
                (MEMO, r),
                (pool, w),
                (x_vault, w),
                (base_vault, w),
                (oracle, w),
                (bitmap, w),
                (arrays[0], w),
                (arrays[1], w),
            ],
        ),
        (
            "DLMM of legacy mints without a bitmap extension",
            legacy,
            dlmm(None),
            vec![
                (DLMM, r),
                (base.mint, r),
                (dlmm_event_authority, r),
                (pool, w),
                (x_vault, w),
                (base_vault, w),
                (oracle, w),
                (arrays[0], w),
                (arrays[1], w),
            ],
        ),
    ];
    let mut checked = 0;
    for (name, x, keys, expected) in cases {
        let block = PoolAccounts::from_keys(pool, x, base, [x_vault, base_vault], keys).unwrap();
        let listed: Vec<_> = block
            .accounts()
            .iter()
            .map(|meta| {
                assert!(!meta.is_signer, "{name}");
                (meta.pubkey, meta.is_writable)
            })
            .collect();
        assert_eq!(listed, expected, "{name}");
        checked += 1;
    }
    assert_eq!(checked, 5);
}

/// A Manifest market whose bids and asks each form a left-leaning chain,
/// best first, with `true` marking a global order, then two free nodes.
fn manifest_book(base: Pubkey, quote: Pubkey, bids: &[bool], asks: &[bool]) -> Vec<u8> {
    let orders = bids.len() + asks.len();
    let mut state = vec![0; 256 + (orders + 2) * 80];
    state[..8].copy_from_slice(&4859840929024028656u64.to_le_bytes());
    for (offset, key) in [
        (16, base),
        (48, quote),
        (80, Pubkey::new_unique()),
        (112, Pubkey::new_unique()),
    ] {
        state[offset..offset + 32].copy_from_slice(key.as_ref());
    }
    let link = |state: &mut Vec<u8>, index: usize, at: usize, to: u32| {
        state[256 + index * 80 + at..][..4].copy_from_slice(&to.to_le_bytes());
    };
    let mut start = 0;
    for (best, side) in [(160, bids), (168, asks)] {
        let first = if side.is_empty() {
            u32::MAX
        } else {
            (start * 80) as u32
        };
        state[best..best + 4].copy_from_slice(&first.to_le_bytes());
        for (position, &global) in side.iter().enumerate() {
            let index = start + position;
            let left = if position + 1 == side.len() {
                u32::MAX
            } else {
                ((index + 1) * 80) as u32
            };
            let parent = if position == 0 {
                u32::MAX
            } else {
                ((index - 1) * 80) as u32
            };
            link(&mut state, index, 0, left);
            link(&mut state, index, 4, u32::MAX);
            link(&mut state, index, 8, parent);
            state[256 + index * 80 + 57] = if global { 3 } else { 0 };
        }
        start += side.len();
    }
    state[176..180].copy_from_slice(&((orders * 80) as u32).to_le_bytes());
    link(&mut state, orders, 0, ((orders + 1) * 80) as u32);
    state
}

#[test]
fn manifest_global_mints_name_the_mints_backing_the_best_orders() {
    let (base, quote) = (Pubkey::new_unique(), SOL);
    let mints = |bids: &[bool], asks: &[bool]| {
        manifest_global_mints(&manifest_book(base, quote, bids, asks)).unwrap()
    };
    assert_eq!(mints(&[false, false], &[false]), Vec::<Pubkey>::new());
    // Quote deposits back bids, base deposits back asks.
    assert_eq!(mints(&[true], &[]), [quote]);
    assert_eq!(mints(&[], &[false, false, true]), [base]);
    assert_eq!(mints(&[false, true], &[true]), [quote, base]);
    // A global order past the scanned depth is not worth its accounts.
    let mut deep = vec![false; 32];
    deep.push(true);
    assert_eq!(mints(&deep, &[]), Vec::<Pubkey>::new());
    deep.remove(0);
    assert_eq!(mints(&deep, &[]), [quote]);
    // A malformed link is an error, not an empty listing.
    let mut broken = manifest_book(base, quote, &[false, false], &[]);
    broken[256..260].copy_from_slice(&81u32.to_le_bytes());
    let error = |state: &[u8]| manifest_global_mints(state).unwrap_err().to_string();
    assert_eq!(error(&broken), "misaligned Manifest node");
    // So is a cycle: the second bid's right link names itself.
    let mut cyclic = manifest_book(base, quote, &[false, false], &[]);
    cyclic[256 + 80 + 4..][..4].copy_from_slice(&80u32.to_le_bytes());
    assert_eq!(error(&cyclic), "cyclic Manifest order tree");
}

/// Bids in a tree rooted at the second best, whose right child is the best
/// and whose left child, the worst, holds the third best as its right child.
/// Each order past the best takes a climb from a right child, a descent to
/// a left subtree's last right child, or both.
#[test]
fn a_manifest_scan_reaches_every_order_of_a_branching_tree() {
    let (base, quote) = (Pubkey::new_unique(), SOL);
    let nil = u32::MAX;
    // Each node's left, right and parent links, best order first.
    let links = [
        [nil, nil, 80],
        [240, 0, nil],
        [nil, nil, 240],
        [nil, 160, 80],
    ];
    for global in 0..links.len() {
        let mut bids = [false; 4];
        bids[global] = true;
        let mut state = manifest_book(base, quote, &bids, &[]);
        for (node, node_links) in links.iter().enumerate() {
            for (field, link) in node_links.iter().enumerate() {
                state[256 + node * 80 + field * 4..][..4].copy_from_slice(&link.to_le_bytes());
            }
        }
        assert_eq!(
            manifest_global_mints(&state).unwrap(),
            [quote],
            "global bid {global}"
        );
    }
}

#[test]
fn a_manifest_block_lists_each_mints_global_once() {
    let wallet = Pubkey::new_unique();
    let x = MintAccounts::ata(&wallet, Pubkey::new_unique(), TOKEN).unwrap();
    let base = MintAccounts::ata(&wallet, SOL, TOKEN).unwrap();
    let state = manifest_book(x.mint, base.mint, &[true], &[true]);
    let plain =
        PoolAccounts::from_state(Pubkey::new_unique(), MANIFEST, &state, x, base, &[], None)
            .unwrap();
    let mints = [base.mint, x.mint];
    let globals = [manifest_global(&base.mint), manifest_global(&x.mint)];
    let listed = plain.clone().with_manifest_globals(&mints).unwrap();
    assert_eq!(listed.accounts()[..5], plain.accounts()[..]);
    let tail: Vec<_> = listed.accounts()[5..]
        .iter()
        .map(|meta| (meta.pubkey, meta.is_writable, meta.is_signer))
        .collect();
    assert_eq!(
        tail,
        globals
            .iter()
            .flatten()
            .map(|key| (*key, true, false))
            .collect::<Vec<_>>()
    );
    assert_eq!(plain.venue(), Venue::Manifest);
    assert_eq!(listed.venue(), Venue::ManifestGlobal);
    // SOL's global and its vault on mainnet.
    assert_eq!(
        manifest_global(&SOL),
        [
            pubkey!("7mR36vj6pvg1U1cRatvUbLG57yqsd1ojLbrgxb6azaQ1"),
            pubkey!("E1mBVQyt7BHK8SaBSfME7usYxx94T4DtHEjbUpEBhZx")
        ]
    );
    // Once per mint of the market, on a Manifest block not already listing
    // them.
    let unlisted = plain.clone().with_manifest_globals(&[]).unwrap();
    assert_eq!(unlisted.accounts(), plain.accounts());
    assert_eq!(unlisted.venue(), Venue::Manifest);
    let error = |pool: &PoolAccounts, mints: &[Pubkey]| {
        pool.clone()
            .with_manifest_globals(mints)
            .unwrap_err()
            .to_string()
    };
    let once_per_mint = "a Manifest market takes one global per mint";
    assert_eq!(error(&plain, &[mints[0], mints[0]]), once_per_mint);
    assert_eq!(
        error(&plain, &[mints[0], mints[1], mints[0]]),
        once_per_mint
    );
    assert_eq!(
        error(&plain, &[mints[0], USDC]),
        "a Manifest market's globals are of its own mints"
    );
    assert_eq!(
        error(&listed, &mints[..1]),
        "only a Manifest market without globals takes them"
    );
}

/// An active Archer market of `base` priced in `quote` with unit lots and
/// scales and six base decimals.
fn archer_market(base: Pubkey, quote: Pubkey, vaults: [Pubkey; 2]) -> Vec<u8> {
    let mut state = vec![0; 272];
    state[..8].copy_from_slice(b"ACHRMKT1");
    for (offset, key) in [(40, base), (72, quote), (104, vaults[0]), (136, vaults[1])] {
        state[offset..offset + 32].copy_from_slice(key.as_ref());
    }
    for offset in [200, 208, 216, 224] {
        state[offset..offset + 8].copy_from_slice(&1u64.to_le_bytes());
    }
    state[256] = 6;
    state
}

#[test]
fn archer_block_lists_the_market_its_vaults_and_writable_books() {
    let request = request();
    let x = MintAccounts::ata(&request.wallet, Pubkey::new_unique(), TOKEN_2022).unwrap();
    let base = request.settlement;
    let market = Pubkey::new_unique();
    let vaults = [Pubkey::new_unique(), Pubkey::new_unique()];
    let state = archer_market(x.mint, base.mint, vaults);
    let books: Vec<_> = (0..20).map(|_| Pubkey::new_unique()).collect();
    let build = |state: &[u8], x: MintAccounts, base: MintAccounts, books: &[Pubkey]| {
        PoolAccounts::from_archer_state(market, state, x, base, books)
    };
    let block = build(&state, x, base, &books[..3]).unwrap();
    let mut expected = vec![
        (ARCHER, false),
        (base.mint, false),
        (market, true),
        (vaults[0], true),
        (vaults[1], true),
    ];
    expected.extend(books[..3].iter().map(|&book| (book, true)));
    assert_eq!(
        block
            .accounts()
            .iter()
            .map(|meta| (meta.pubkey, meta.is_writable))
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(block.venue(), Venue::Archer);
    assert_eq!(archer::book_count(block.accounts()), 3);
    // The declared x side may be the market's quote.
    let flipped = build(&state, base, x, &books[..3]).unwrap();
    assert_eq!(flipped.accounts()[1].pubkey, x.mint);
    assert_eq!(flipped.accounts()[3].pubkey, vaults[1]);
    assert_eq!(flipped.accounts()[4].pubkey, vaults[0]);
    let error = |base: MintAccounts, books: &[Pubkey]| {
        build(&state, x, base, books).unwrap_err().to_string()
    };
    // A block holds 24 accounts: at most 19 books after the five fixed.
    let book_count = "V10 requires 1..=19 books per Archer market";
    assert_eq!(error(base, &[]), book_count);
    assert_eq!(
        build(&state, x, base, &books[..19])
            .unwrap()
            .accounts()
            .len(),
        24
    );
    assert_eq!(error(base, &books), book_count);
    // The executor rejects a repeated or aliased book.
    for alias in [books[0], market, vaults[0], vaults[1]] {
        assert_eq!(
            error(base, &[books[0], books[1], alias]),
            format!("Archer book {alias} repeats a listed account")
        );
    }
    assert!(build(&state, x, base, &[books[0], books[1], books[2]]).is_ok());
    // The generic builder cannot list the books.
    assert_eq!(
        PoolAccounts::from_state(market, ARCHER, &state, x, base, &books[..3], None)
            .unwrap_err()
            .to_string(),
        "Archer requires from_archer_state with its maker books"
    );
    assert_eq!(
        error(
            MintAccounts::ata(&request.wallet, USDC, TOKEN).unwrap(),
            &books[..3]
        ),
        "pool does not match the declared mint pair"
    );
}

/// The executor rejects a transaction listing a market with a zero lot or
/// scale, and quotes no fill from one that is inactive, charges a negative
/// taker fee or whose price denominator 10^base_decimals * quote_scale
/// overflows u128; the builder refuses each.
#[test]
fn archer_markets_the_executor_cannot_quote_are_refused() {
    let wallet = Pubkey::new_unique();
    let x = MintAccounts::ata(&wallet, Pubkey::new_unique(), TOKEN).unwrap();
    let base = MintAccounts::ata(&wallet, SOL, TOKEN).unwrap();
    let state = archer_market(
        x.mint,
        base.mint,
        [Pubkey::new_unique(), Pubkey::new_unique()],
    );
    let books = [Pubkey::new_unique()];
    let market = Pubkey::new_unique();
    let load = |state: &[u8]| PoolAccounts::from_archer_state(market, state, x, base, &books);
    let with = |edit: &dyn Fn(&mut Vec<u8>)| {
        let mut state = state.clone();
        edit(&mut state);
        state
    };
    let set_u64 = |offset: usize, value: u64| {
        with(&move |state: &mut Vec<u8>| {
            state[offset..offset + 8].copy_from_slice(&value.to_le_bytes())
        })
    };
    let set_i32 = |offset: usize, value: i32| {
        with(&move |state: &mut Vec<u8>| {
            state[offset..offset + 4].copy_from_slice(&value.to_le_bytes())
        })
    };
    let set_u8 = |offset: usize, value: u8| with(&move |state: &mut Vec<u8>| state[offset] = value);
    let accepted = [
        state.clone(),
        // A maker fee may be a rebate.
        set_i32(248, -1),
        set_i32(252, 0),
        // 10^20 * 1 fits u128, 10^19 * (2^64 - 1) too.
        set_u8(256, 20),
        with(&|state: &mut Vec<u8>| {
            state[256] = 19;
            state[224..232].copy_from_slice(&u64::MAX.to_le_bytes());
        }),
    ];
    for state in &accepted {
        load(state).unwrap();
    }
    let not_a_market = format!("{market} is not an Archer market");
    let zero_unit = format!("Archer market {market} has a zero lot size or scale");
    let overflow = format!("Archer market {market}'s price denominator overflows");
    let refused = [
        (
            "other discriminator",
            with(&|state: &mut Vec<u8>| state[0] ^= 1),
            &not_a_market,
        ),
        (
            "short",
            with(&|state: &mut Vec<u8>| state.truncate(271)),
            &not_a_market,
        ),
        (
            "long",
            with(&|state: &mut Vec<u8>| state.push(0)),
            &not_a_market,
        ),
        (
            "inactive",
            set_u8(258, 1),
            &format!("Archer market {market} is not active"),
        ),
        ("zero base lot", set_u64(200, 0), &zero_unit),
        ("zero quote lot", set_u64(208, 0), &zero_unit),
        ("zero price scale", set_u64(216, 0), &zero_unit),
        ("zero quote scale denominator", set_u64(224, 0), &zero_unit),
        (
            "negative taker fee",
            set_i32(252, -1),
            &format!("Archer market {market} has a negative taker fee"),
        ),
        // 10^39 overflows u128, as does 10^20 * (2^64 - 1).
        ("10^39", set_u8(256, 39), &overflow),
        (
            "10^20 * (2^64 - 1)",
            with(&|state: &mut Vec<u8>| {
                state[256] = 20;
                state[224..232].copy_from_slice(&u64::MAX.to_le_bytes());
            }),
            &overflow,
        ),
    ];
    for (case, state, expected) in &refused {
        assert_eq!(&load(state).unwrap_err().to_string(), *expected, "{case}");
    }
}

#[test]
fn a_futarchy_dao_lists_its_spot_pool_and_nothing_else() {
    let wallet = Pubkey::new_unique();
    let mint = |mint, program| MintAccounts::ata(&wallet, mint, program).unwrap();
    let target = mint(Pubkey::new_unique(), TOKEN);
    let usdc = mint(USDC, TOKEN);
    let dao = Pubkey::new_unique();
    let vaults = [Pubkey::new_unique(), Pubkey::new_unique()];
    // A Dao in the spot state: its discriminator, the state byte, then the
    // base mint, quote mint and their vaults after the spot pool.
    let mut data = vec![0u8; 1264];
    data[..8].copy_from_slice(&[163, 9, 47, 31, 52, 85, 197, 49]);
    data[157..189].copy_from_slice(target.mint.as_ref());
    data[189..221].copy_from_slice(usdc.mint.as_ref());
    data[221..253].copy_from_slice(vaults[0].as_ref());
    data[253..285].copy_from_slice(vaults[1].as_ref());
    let load = |data: &[u8], x: MintAccounts, base: MintAccounts| {
        PoolAccounts::from_state(dao, FUTARCHY, data, x, base, &[], None)
    };
    let event_authority = Pubkey::find_program_address(&[b"__event_authority"], &FUTARCHY).0;
    assert_eq!(
        event_authority,
        pubkey!("DGEympSS4qLvdr9r3uGHTfACdN8snShk4iGdJtZPxuBC")
    );
    // The program, the declared base mint, the Dao, the vaults in X and
    // base order, and the event authority, whichever side the target is.
    for (x, base, x_vault, base_vault) in [
        (target, usdc, vaults[0], vaults[1]),
        (usdc, target, vaults[1], vaults[0]),
    ] {
        let pool = load(&data, x, base).unwrap();
        let listed: Vec<_> = pool
            .accounts()
            .iter()
            .map(|meta| (meta.pubkey, meta.is_writable))
            .collect();
        assert_eq!(
            listed,
            [
                (FUTARCHY, false),
                (base.mint, false),
                (dao, true),
                (x_vault, true),
                (base_vault, true),
                (event_authority, false),
            ]
        );
        assert_eq!(pool.venue(), Venue::Futarchy);
    }
    let error = |data: &[u8], x, base| load(data, x, base).unwrap_err().to_string();
    // One token program serves both mints, so Token-2022 mints are refused.
    let legacy_only = "V10 Futarchy requires legacy token mints";
    assert_eq!(
        error(&data, mint(target.mint, TOKEN_2022), usdc),
        legacy_only
    );
    assert_eq!(error(&data, target, mint(USDC, TOKEN_2022)), legacy_only);
    // A live proposal, another account of the program, a foreign pair.
    let not_spot = "not a Futarchy Dao with a single spot pool";
    let mut proposal = data.clone();
    proposal[8] = 1;
    assert_eq!(error(&proposal, target, usdc), not_spot);
    let mut other = data.clone();
    other[0] ^= 1;
    assert_eq!(error(&other, target, usdc), not_spot);
    assert_eq!(
        error(&data, mint(Pubkey::new_unique(), TOKEN), usdc),
        "pool does not match the declared mint pair"
    );
    let account = solana_sdk::account::Account {
        owner: FUTARCHY,
        data,
        ..Default::default()
    };
    assert_eq!(
        crate::arrays::array_walks(dao, &account, |_| None).unwrap(),
        [vec![], vec![]]
    );
}

#[test]
fn a_damm_v2_pool_is_quoted_by_how_it_collects_fees() {
    let wallet = Pubkey::new_unique();
    let target = MintAccounts::ata(&wallet, Pubkey::new_unique(), TOKEN).unwrap();
    let sol = MintAccounts::ata(&wallet, SOL, TOKEN).unwrap();
    let vaults = [Pubkey::new_unique(), Pubkey::new_unique()];
    // The pool's mints and vaults, and its collect-fee mode after the
    // liquidity, price range and activation fields.
    let mut data = vec![0u8; 1112];
    data[168..200].copy_from_slice(target.mint.as_ref());
    data[200..232].copy_from_slice(sol.mint.as_ref());
    data[232..264].copy_from_slice(vaults[0].as_ref());
    data[264..296].copy_from_slice(vaults[1].as_ref());
    let venue = |mode: u8| {
        let mut data = data.clone();
        data[484] = mode;
        PoolAccounts::from_state(Pubkey::new_unique(), DAMMV2, &data, target, sol, &[], None)
            .unwrap()
            .venue()
    };
    assert_eq!(venue(0), Venue::Dammv2);
    assert_eq!(venue(1), Venue::Dammv2);
    assert_eq!(venue(2), Venue::Dammv2Compounding);
    let truncated = PoolAccounts::from_state(
        Pubkey::new_unique(),
        DAMMV2,
        &data[..484],
        target,
        sol,
        &[],
        None,
    );
    assert_eq!(truncated.unwrap_err().to_string(), "truncated pool account");
    for (compounding, expected) in [(false, Venue::Dammv2), (true, Venue::Dammv2Compounding)] {
        let keys = PoolKeys::DammV2 { compounding };
        let keyed = PoolAccounts::from_keys(Pubkey::new_unique(), target, sol, vaults, keys);
        assert_eq!(keyed.unwrap().venue(), expected);
    }
}

/// A Whirlpool's state with tick spacing 16 and `fee_tier_index`, then its
/// mints and vaults. A fee-tier index other than the tick spacing names an
/// adaptive-fee tier, such as 1030 for spacing 16.
fn whirlpool_state(mints: [Pubkey; 2], fee_tier_index: u16) -> Vec<u8> {
    let mut data = vec![0u8; 653];
    data[41..43].copy_from_slice(&16u16.to_le_bytes());
    data[43..45].copy_from_slice(&fee_tier_index.to_le_bytes());
    for (offset, key) in [
        (101, mints[0]),
        (133, Pubkey::new_unique()),
        (181, mints[1]),
        (213, Pubkey::new_unique()),
    ] {
        data[offset..offset + 32].copy_from_slice(key.as_ref());
    }
    data
}

#[test]
fn a_whirlpools_adaptive_fee_comes_from_its_state_or_its_keys() {
    let wallet = Pubkey::new_unique();
    let target = MintAccounts::ata(&wallet, Pubkey::new_unique(), TOKEN).unwrap();
    let sol = MintAccounts::ata(&wallet, SOL, TOKEN).unwrap();
    let arrays = [Pubkey::new_unique()];
    let adaptive_fee = |fee_tier_index: u16| {
        let data = whirlpool_state([target.mint, sol.mint], fee_tier_index);
        let pool = PoolAccounts::from_state(
            Pubkey::new_unique(),
            WHIRLPOOL,
            &data,
            target,
            sol,
            &arrays,
            None,
        )
        .unwrap();
        assert_eq!(pool.venue(), Venue::Whirlpool);
        pool.whirlpool_adaptive_fee()
    };
    assert_eq!(adaptive_fee(16), Some(false));
    assert_eq!(adaptive_fee(1030), Some(true));
    for adaptive_fee in [None, Some(false), Some(true)] {
        let keyed = PoolAccounts::from_keys(
            Pubkey::new_unique(),
            target,
            sol,
            [Pubkey::new_unique(), Pubkey::new_unique()],
            PoolKeys::Whirlpool {
                arrays: &arrays,
                adaptive_fee,
            },
        )
        .unwrap();
        assert_eq!(keyed.whirlpool_adaptive_fee(), adaptive_fee);
    }
}
