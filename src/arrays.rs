//! The tick or bin arrays a concentrated pool lists for the executor.
//!
//! The executor picks the swap direction on chain, so a pool lists both
//! directions' arrays, the price-down direction's first, each in the order
//! its swap walks them. The executor takes one to `MAX_ARRAYS` arrays and
//! sizes the trade to the ones it is given, but rejects the whole
//! instruction if a listed array holds no account.
use crate::clmm;
use crate::dlmm_pro::{self, DLMM_PRO};
use crate::v10 as abi;
use anyhow::{ensure, Result};
use solana_sdk::{account::Account, pubkey::Pubkey};

/// Arrays each swap direction lists at most.
const ARRAYS_PER_DIRECTION: usize = 2;
const _: () = assert!(2 * ARRAYS_PER_DIRECTION <= abi::MAX_ARRAYS);

const WHIRLPOOL_DISCRIMINATOR: [u8; 8] = [0x3f, 0x95, 0xd1, 0x0c, 0xe1, 0x80, 0x63, 0x09];
const WHIRLPOOL_LEN: usize = 653;
const WHIRLPOOL_TICKS_PER_ARRAY: i32 = 88;
const WHIRLPOOL_MIN_TICK: i32 = -443_636;
const WHIRLPOOL_MAX_TICK: i32 = 443_636;
const DLMM_PAIR_DISCRIMINATOR: [u8; 8] = [0x21, 0x0b, 0x31, 0x62, 0xb5, 0x65, 0xb1, 0x0d];
const DLMM_PAIR_LEN: usize = 904;
const DLMM_BINS_PER_ARRAY: i32 = 70;

/// Each swap direction's first `ARRAYS_PER_DIRECTION` arrays, price-down
/// first, in the order the venue's swap walks them. `fetched` returns an
/// existing account fetched so far. A pool without arrays walks none.
pub(crate) fn array_walks<'a>(
    pool: Pubkey,
    state: &Account,
    fetched: impl Fn(&Pubkey) -> Option<&'a Account>,
) -> Result<[Vec<Pubkey>; 2]> {
    match state.owner {
        abi::CLMM | abi::PANCAKESWAP | abi::BYREAL => clmm_walks(pool, state, fetched),
        abi::WHIRLPOOL => whirlpool_walks(pool, state),
        abi::DLMM => dlmm_walks(pool, state),
        DLMM_PRO => dlmm_pro_walks(pool, state, fetched),
        _ => Ok(Default::default()),
    }
}

/// The arrays the bitmaps mark as initialized, the only ones Raydium's swap
/// enters. A direction without liquidity walks none. Until the pool's bitmap
/// extension is `fetched`, each walk is a prefix of the one it steers.
fn clmm_walks<'a>(
    pool: Pubkey,
    state: &Account,
    fetched: impl Fn(&Pubkey) -> Option<&'a Account>,
) -> Result<[Vec<Pubkey>; 2]> {
    let clmm = clmm::Pool::decode(pool, &state.data)?;
    let extension = abi::bitmap_extension(pool, state.owner)
        .and_then(|key| fetched(&key))
        .and_then(|account| clmm::Extension::decode(pool, state.owner, account));
    Ok([true, false].map(|zero_for_one| {
        clmm.initialized_starts(extension.as_ref(), zero_for_one)
            .take(ARRAYS_PER_DIRECTION)
            .map(|start| abi::clmm_tick_array(pool, state.owner, start))
            .collect()
    }))
}

/// The arrays a Whirlpool swap walks: from the one holding the current
/// tick, or for a swap up the tick one spacing above it, while Orca accepts
/// their starts.
fn whirlpool_walks(pool: Pubkey, state: &Account) -> Result<[Vec<Pubkey>; 2]> {
    let data = &state.data;
    ensure!(
        data.len() >= WHIRLPOOL_LEN && data[..8] == WHIRLPOOL_DISCRIMINATOR,
        "invalid Whirlpool {pool}"
    );
    let spacing = i32::from(u16::from_le_bytes(abi::read(
        data,
        abi::WHIRLPOOL_TICK_SPACING_OFFSET,
    )?));
    let tick = i32::from_le_bytes(abi::read(data, 81)?);
    ensure!(spacing > 0, "invalid Whirlpool {pool}");
    let width = WHIRLPOOL_TICKS_PER_ARRAY * spacing;
    Ok([true, false].map(|a_to_b| {
        let (shift, step) = if a_to_b { (0, -1) } else { (spacing, 1) };
        let first = tick.checked_add(shift).map(|tick| tick.div_euclid(width));
        (0..)
            .map_while(|offset| {
                let start = first?.checked_add(offset * step)?.checked_mul(width)?;
                whirlpool_start_is_valid(start, width).then_some(start)
            })
            .take(ARRAYS_PER_DIRECTION)
            .map(|start| abi::whirlpool_tick_array(pool, start))
            .collect()
    }))
}

/// Orca's rule for an array start that is a multiple of `width`: within the
/// tick range, or the start of the array holding the minimum tick.
fn whirlpool_start_is_valid(start: i32, width: i32) -> bool {
    match start {
        WHIRLPOOL_MIN_TICK..=WHIRLPOOL_MAX_TICK => true,
        _ => start == WHIRLPOOL_MIN_TICK - (WHIRLPOOL_MIN_TICK % width + width),
    }
}

/// The arrays next to each other from the active one. The executor quotes
/// bin by bin from the active bin and stops at the first whose array is not
/// listed, so it cannot cross an unlisted array, even one without liquidity.
fn dlmm_walks(pool: Pubkey, state: &Account) -> Result<[Vec<Pubkey>; 2]> {
    let data = &state.data;
    ensure!(
        data.len() >= DLMM_PAIR_LEN && data[..8] == DLMM_PAIR_DISCRIMINATOR,
        "invalid DLMM pair {pool}"
    );
    let active_id = i32::from_le_bytes(abi::read(data, 76)?);
    let active = active_id.div_euclid(DLMM_BINS_PER_ARRAY);
    Ok([-1, 1].map(|step| {
        (0..ARRAYS_PER_DIRECTION as i32)
            .map(|offset| abi::dlmm_bin_array(pool, (active + step * offset).into()))
            .collect()
    }))
}

/// The arrays the pool's bitmaps mark as holding liquidity. A pool flagged
/// as having a bitmap extension walks nothing until the extension is
/// `fetched`.
fn dlmm_pro_walks<'a>(
    pool: Pubkey,
    state: &Account,
    fetched: impl Fn(&Pubkey) -> Option<&'a Account>,
) -> Result<[Vec<Pubkey>; 2]> {
    let extension = dlmm_pro::requires_bitmap_extension(&state.data)
        .then(|| dlmm_pro::bitmap_extension(&pool))
        .and_then(|key| fetched(&key))
        .map(|account| account.data.as_slice());
    dlmm_pro::swap_walks(&pool, &state.data, extension, ARRAYS_PER_DIRECTION)
}

/// The arrays a pool lists: each direction's `walks` up to its first array
/// that does not `exist`, where its swap stops, without repeats. Each
/// direction keeps its walk order: a DLMM Pro swap scans the list once and
/// never returns to an array listed before one it took.
pub(crate) fn listed_arrays(
    walks: &[Vec<Pubkey>; 2],
    exists: impl Fn(&Pubkey) -> bool,
) -> Vec<Pubkey> {
    let mut arrays = Vec::new();
    for walk in walks {
        for array in walk.iter().take_while(|array| exists(array)) {
            if !arrays.contains(array) {
                arrays.push(*array);
            }
        }
    }
    arrays
}
