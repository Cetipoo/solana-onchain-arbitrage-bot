//! Meteora DLMM Pro pools: the executor block's fixed accounts and the tick
//! arrays each swap direction walks, found in the pool's own bitmap.
//!
//! The tick-array traversal follows github.com/MeteoraAg/dlmm-pro-sdk
//! `commons/src`, distributed under this license:
//!
//! MIT License
//!
//! Copyright (c) 2026 Meteora
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.
use crate::v10::read;
use anyhow::{ensure, Result};
use solana_sdk::{pubkey, pubkey::Pubkey};

pub const DLMM_PRO: Pubkey = pubkey!("dPro1D5wiqnGMH8vMDG23jh35btTHoWdyqXjqkqxe8n");
/// The `[b"pool_auth"]` PDA, which owns every pool's reserves.
pub const DLMM_PRO_POOL_AUTHORITY: Pubkey = pubkey!("FSUXyUKbTJ4A5Ccz5M6Uo4J2myc3RsDKTPU9vZwyeXru");
/// The `[b"__event_authority"]` PDA.
pub const DLMM_PRO_EVENT_AUTHORITY: Pubkey =
    pubkey!("6D8NF9HL6eTrheURXz9fLvrL1L7Uzfy5JAA2bCAUEN13");

/// Shared with DAMM v2 and other Anchor `Pool` accounts; the owner tells
/// them apart.
pub const POOL_DISCRIMINATOR: [u8; 8] = [241, 154, 109, 4, 17, 177, 109, 188];
/// Account offsets: the pool struct's offsets plus the 8-byte discriminator.
const POOL_LEN: usize = 648;
const BITMAP: usize = 408;
const TICK_CURRENT: usize = 560;
const TICK_SPACING: usize = 564;
const ACTIVATION_TYPE: usize = 573;
const COLLECT_FEE_MODE: usize = 574;
const EXTENSIONS: usize = 575;
const BITMAP_EXTENSION_INITIALIZED: usize = 576;
const REWARD_EXTENSION: u8 = 1;
const ACTIVATION_EXTENSION: u8 = 2;
const REWARD_EXTENSION_LEN: usize = 352;
const ACTIVATION_EXTENSION_LEN: usize = 128;
const TICKS_PER_ARRAY: i64 = 128;
/// The pool's bitmap covers arrays `-BITMAP_SIZE..BITMAP_SIZE`.
const BITMAP_SIZE: i32 = 512;
const EXTENDED_BITMAP_SIZE: i32 = 7 * BITMAP_SIZE;
pub const BITMAP_EXTENSION_DISCRIMINATOR: [u8; 8] = [60, 150, 36, 219, 97, 128, 139, 153];
pub const BITMAP_EXTENSION_LEN: usize = 808;

/// Token-2022 mint layout: the account-type byte after the 165-byte base
/// account, then type-length-value extensions.
const MINT_ACCOUNT_TYPE: usize = 165;
const MINT_ACCOUNT: u8 = 1;
const MINT_EXTENSIONS: usize = 166;
const TRANSFER_HOOK_EXTENSION: u16 = 14;

pub fn bitmap_extension(pool: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"bitmap", pool.as_ref()], &DLMM_PRO).0
}

pub fn requires_bitmap_extension(data: &[u8]) -> bool {
    data.get(BITMAP_EXTENSION_INITIALIZED) == Some(&1)
}

/// Whether a Token-2022 mint's account names a transfer-hook program. This
/// client lists no hook accounts, so a swap of such a mint would fail.
pub fn has_transfer_hook(mint: &[u8]) -> Result<bool> {
    if mint.len() <= MINT_ACCOUNT_TYPE {
        return Ok(false);
    }
    ensure!(
        mint[MINT_ACCOUNT_TYPE] == MINT_ACCOUNT,
        "not a Token-2022 mint account"
    );
    let mut offset = MINT_EXTENSIONS;
    while offset + 4 <= mint.len() {
        let kind = u16::from_le_bytes(read(mint, offset)?);
        let len = u16::from_le_bytes(read(mint, offset + 2)?) as usize;
        let value = mint
            .get(offset + 4..offset + 4 + len)
            .ok_or_else(|| anyhow::anyhow!("truncated Token-2022 mint extension"))?;
        if kind == TRANSFER_HOOK_EXTENSION {
            // The authority, then the hook program; a zero program is no hook.
            let program = value
                .get(32..64)
                .ok_or_else(|| anyhow::anyhow!("truncated transfer-hook extension"))?;
            return Ok(program.iter().any(|&byte| byte != 0));
        }
        offset += 4 + len;
    }
    Ok(false)
}

pub fn oracle(pool: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"oracle", pool.as_ref()], &DLMM_PRO).0
}

pub fn tick_array(pool: &Pubkey, index: i32) -> Pubkey {
    Pubkey::find_program_address(
        &[b"tick_array", pool.as_ref(), &index.to_le_bytes()],
        &DLMM_PRO,
    )
    .0
}

/// Checks the pool layout and parameter discriminants the executor reads.
pub fn check_tradable(pool: &Pubkey, data: &[u8]) -> Result<()> {
    ensure!(data.len() >= POOL_LEN, "truncated DLMM Pro pool {pool}");
    ensure!(
        data[..8] == POOL_DISCRIMINATOR,
        "{pool} is not a DLMM Pro pool"
    );
    ensure!(
        data[COLLECT_FEE_MODE] <= 1 && data[ACTIVATION_TYPE] <= 1,
        "DLMM Pro pool {pool} has an unknown fee mode or activation mode"
    );
    ensure!(
        data[EXTENSIONS] & !(REWARD_EXTENSION | ACTIVATION_EXTENSION) == 0,
        "DLMM Pro pool {pool} has unsupported extensions {:#04x}",
        data[EXTENSIONS]
    );
    if data[EXTENSIONS] & REWARD_EXTENSION != 0 {
        ensure!(
            data.len() >= POOL_LEN + REWARD_EXTENSION_LEN,
            "DLMM Pro pool {pool} has a truncated rewards extension"
        );
    }
    if data[EXTENSIONS] & ACTIVATION_EXTENSION != 0 {
        let offset = POOL_LEN
            + if data[EXTENSIONS] & REWARD_EXTENSION != 0 {
                REWARD_EXTENSION_LEN
            } else {
                0
            };
        ensure!(
            data.len() >= offset + ACTIVATION_EXTENSION_LEN,
            "DLMM Pro pool {pool} has a truncated activation extension"
        );
    }
    ensure!(
        u16::from_le_bytes(read(data, TICK_SPACING)?) > 0,
        "DLMM Pro pool {pool} has a tick spacing of 0"
    );
    Ok(())
}

/// Each direction's first `per_side` arrays holding liquidity, price-down
/// (selling x for y) first, in the order its swap walks them from the
/// current tick's array. `extension` is the pool's bitmap extension, which
/// a pool flagged as having one requires.
pub fn swap_walks(
    pool: &Pubkey,
    data: &[u8],
    extension: Option<&[u8]>,
    per_side: usize,
) -> Result<[Vec<Pubkey>; 2]> {
    check_tradable(pool, data)?;
    let extension = if requires_bitmap_extension(data) {
        let extension = extension
            .ok_or_else(|| anyhow::anyhow!("DLMM Pro pool {pool} requires its bitmap extension"))?;
        ensure!(
            extension.len() >= BITMAP_EXTENSION_LEN
                && extension[..8] == BITMAP_EXTENSION_DISCRIMINATOR
                && extension[8..40] == pool.to_bytes(),
            "invalid DLMM Pro bitmap extension for {pool}"
        );
        Some(extension)
    } else {
        None
    };
    Ok(walk_indexes(data, extension, per_side)?.map(|indexes| {
        indexes
            .into_iter()
            .map(|index| tick_array(pool, index))
            .collect()
    }))
}

fn walk_indexes(data: &[u8], extension: Option<&[u8]>, per_side: usize) -> Result<[Vec<i32>; 2]> {
    let spacing = u16::from_le_bytes(read(data, TICK_SPACING)?);
    let tick = i32::from_le_bytes(read(data, TICK_CURRENT)?);
    let start = i64::from(tick).div_euclid(i64::from(spacing) * TICKS_PER_ARRAY) as i32;
    let bitmap = Bitmap {
        internal: read(data, BITMAP)?,
        extension,
    };
    ensure!(
        extension.is_none() || (-EXTENDED_BITMAP_SIZE..EXTENDED_BITMAP_SIZE).contains(&start),
        "DLMM Pro start array is outside its bitmap extension"
    );
    Ok([true, false].map(|swap_for_y| bitmap.indexes(swap_for_y, start).take(per_side).collect()))
}

struct Bitmap<'a> {
    internal: [u8; 128],
    extension: Option<&'a [u8]>,
}

impl Bitmap<'_> {
    fn has(&self, index: i32) -> bool {
        let (bytes, bit) = if (-BITMAP_SIZE..BITMAP_SIZE).contains(&index) {
            (&self.internal[..], (index + BITMAP_SIZE) as usize)
        } else if let Some(extension) = self.extension {
            if index >= BITMAP_SIZE {
                (&extension[40..424], (index - BITMAP_SIZE) as usize)
            } else {
                (&extension[424..808], (-index - BITMAP_SIZE - 1) as usize)
            }
        } else {
            return false;
        };
        bytes[bit / 8] & (1 << (bit % 8)) != 0
    }

    fn next_index(&self, swap_for_y: bool, from: i32) -> Option<i32> {
        let size = if self.extension.is_some() {
            EXTENDED_BITMAP_SIZE
        } else {
            BITMAP_SIZE
        };
        let mut index = if (-size..size).contains(&from) {
            from
        } else if swap_for_y && from >= size {
            size - 1
        } else if !swap_for_y && from < -size {
            -size
        } else {
            return None;
        };
        loop {
            if self.has(index) {
                return Some(index);
            }
            index += if swap_for_y { -1 } else { 1 };
            if !(-size..size).contains(&index) {
                return None;
            }
        }
    }

    fn indexes(&self, swap_for_y: bool, start: i32) -> impl Iterator<Item = i32> + '_ {
        std::iter::successors(self.next_index(swap_for_y, start), move |&index| {
            self.next_index(swap_for_y, if swap_for_y { index - 1 } else { index + 1 })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrays::listed_arrays;

    fn pool(tick: i32, spacing: u16, arrays: &[i32]) -> Vec<u8> {
        let mut data = vec![0; POOL_LEN];
        data[..8].copy_from_slice(&POOL_DISCRIMINATOR);
        data[TICK_CURRENT..TICK_CURRENT + 4].copy_from_slice(&tick.to_le_bytes());
        data[TICK_SPACING..TICK_SPACING + 2].copy_from_slice(&spacing.to_le_bytes());
        for index in arrays {
            let bit = (index + BITMAP_SIZE) as usize;
            data[BITMAP + bit / 8] |= 1 << (bit % 8);
        }
        data
    }

    /// The SDK's `tick_arrays_for_swap` cases: bitmap {-3, -1, 0, 2} at tick
    /// 0, spacing 1.
    #[test]
    fn walks_match_the_sdks_traversal() {
        let data = pool(0, 1, &[-3, -1, 0, 2]);
        assert_eq!(
            walk_indexes(&data, None, 3).unwrap(),
            [vec![0, -1, -3], vec![0, 2]]
        );
        assert_eq!(
            walk_indexes(&data, None, 2).unwrap(),
            [vec![0, -1], vec![0, 2]]
        );
        assert_eq!(walk_indexes(&data, None, 6).unwrap()[0], [0, -1, -3]);
    }

    #[test]
    fn walks_start_at_the_current_ticks_array_and_cover_the_bitmap_edges() {
        // Tick -1 at spacing 10 is in array -1, not 0.
        let data = pool(-1, 10, &[-512, -1, 0, 511]);
        assert_eq!(
            walk_indexes(&data, None, 6).unwrap(),
            [vec![-1, -512], vec![-1, 0, 511]]
        );
        // Outside the bitmap, a walk toward it resumes at its edge and one
        // leading away finds nothing.
        let above = pool(512 * 128, 1, &[511, 3]);
        assert_eq!(
            walk_indexes(&above, None, 6).unwrap(),
            [vec![511, 3], vec![]]
        );
        let below = pool(-513 * 128, 1, &[-512, 7]);
        assert_eq!(
            walk_indexes(&below, None, 6).unwrap(),
            [vec![], vec![-512, 7]]
        );
    }

    #[test]
    fn one_list_holds_each_direction_in_swap_order_without_repeats() {
        let key = Pubkey::new_unique();
        let data = pool(0, 1, &[-3, -1, 0, 2, 5]);
        let arrays = |indexes: &[i32]| -> Vec<Pubkey> {
            indexes
                .iter()
                .map(|&index| tick_array(&key, index))
                .collect()
        };
        let walks = swap_walks(&key, &data, None, 2).unwrap();
        assert_eq!(walks, [arrays(&[0, -1]), arrays(&[0, 2])]);
        assert_eq!(listed_arrays(&walks, |_| true), arrays(&[0, -1, 2]));
        // A missing array ends its direction's walk, even when a later one
        // exists.
        let listed_without = |index| {
            let missing = tick_array(&key, index);
            listed_arrays(&walks, |array| *array != missing)
        };
        assert_eq!(listed_without(-1), arrays(&[0, 2]));
        assert_eq!(listed_without(0), arrays(&[]));
    }

    fn walk_error(key: &Pubkey, data: &[u8], extension: Option<&[u8]>) -> String {
        swap_walks(key, data, extension, 2).unwrap_err().to_string()
    }

    #[test]
    fn pools_the_executor_rejects_list_no_arrays() {
        let key = Pubkey::new_unique();
        let mut data = pool(0, 1, &[0]);
        assert!(swap_walks(&key, &data, None, 2).is_ok());
        data[EXTENSIONS] = REWARD_EXTENSION;
        assert_eq!(
            walk_error(&key, &data, None),
            format!("DLMM Pro pool {key} has a truncated rewards extension")
        );
        data.resize(POOL_LEN + REWARD_EXTENSION_LEN, 0);
        assert!(swap_walks(&key, &data, None, 2).is_ok());
        data[EXTENSIONS] = REWARD_EXTENSION | 4;
        assert_eq!(
            walk_error(&key, &data, None),
            format!("DLMM Pro pool {key} has unsupported extensions 0x05")
        );
        data[EXTENSIONS] = REWARD_EXTENSION | ACTIVATION_EXTENSION;
        assert_eq!(
            walk_error(&key, &data, None),
            format!("DLMM Pro pool {key} has a truncated activation extension")
        );
        data.resize(
            POOL_LEN + REWARD_EXTENSION_LEN + ACTIVATION_EXTENSION_LEN,
            0,
        );
        assert!(swap_walks(&key, &data, None, 2).is_ok());
        data[EXTENSIONS] = 0;
        data[BITMAP_EXTENSION_INITIALIZED] = 1;
        assert_eq!(
            walk_error(&key, &data, None),
            format!("DLMM Pro pool {key} requires its bitmap extension")
        );
        data[BITMAP_EXTENSION_INITIALIZED] = 0;
        for (offset, accepted, rejected) in [(COLLECT_FEE_MODE, 1, 2), (ACTIVATION_TYPE, 1, 2)] {
            data[offset] = accepted;
            assert!(swap_walks(&key, &data, None, 2).is_ok());
            data[offset] = rejected;
            assert_eq!(
                walk_error(&key, &data, None),
                format!("DLMM Pro pool {key} has an unknown fee mode or activation mode")
            );
            data[offset] = 0;
        }
        assert_eq!(
            walk_error(&key, &pool(0, 1, &[0])[..POOL_LEN - 1], None),
            format!("truncated DLMM Pro pool {key}")
        );
        assert_eq!(
            walk_error(&key, &pool(0, 0, &[0]), None),
            format!("DLMM Pro pool {key} has a tick spacing of 0")
        );
        let mut other = pool(0, 1, &[0]);
        other[0] ^= 1;
        assert_eq!(
            walk_error(&key, &other, None),
            format!("{key} is not a DLMM Pro pool")
        );
    }

    #[test]
    fn extension_walks_cross_both_inline_edges_and_validate_the_binding() {
        let key = Pubkey::new_unique();
        let mut data = pool(0, 1, &[-512, 0, 511]);
        data[BITMAP_EXTENSION_INITIALIZED] = 1;
        let mut extension = vec![0; BITMAP_EXTENSION_LEN];
        extension[..8].copy_from_slice(&BITMAP_EXTENSION_DISCRIMINATOR);
        extension[8..40].copy_from_slice(key.as_ref());
        for (offset, bit) in [(40, 0), (40, 3071), (424, 0), (424, 3071)] {
            extension[offset + bit / 8] |= 1 << (bit % 8);
        }
        let walks = swap_walks(&key, &data, Some(&extension), 6).unwrap();
        assert_eq!(
            walks[0],
            [0, -512, -513, -3584].map(|i| tick_array(&key, i))
        );
        assert_eq!(walks[1], [0, 511, 512, 3583].map(|i| tick_array(&key, i)));
        assert_eq!(
            walk_error(&key, &data, None),
            format!("DLMM Pro pool {key} requires its bitmap extension")
        );
        extension[8] ^= 1;
        assert_eq!(
            walk_error(&key, &data, Some(&extension)),
            format!("invalid DLMM Pro bitmap extension for {key}")
        );
        extension[8] ^= 1;
        for (start, expected) in [
            (3583, [vec![3583, 512, 511, 0, -512, -513], vec![3583]]),
            (-3584, [vec![-3584], vec![-3584, -513, -512, 0, 511, 512]]),
        ] {
            data[TICK_CURRENT..TICK_CURRENT + 4].copy_from_slice(&(start * 128i32).to_le_bytes());
            assert_eq!(walk_indexes(&data, Some(&extension), 6).unwrap(), expected);
        }
        data[TICK_CURRENT..TICK_CURRENT + 4].copy_from_slice(&(3584 * 128i32).to_le_bytes());
        assert_eq!(
            walk_error(&key, &data, Some(&extension)),
            "DLMM Pro start array is outside its bitmap extension"
        );
        data[BITMAP_EXTENSION_INITIALIZED] = 2;
        assert_eq!(
            swap_walks(&key, &data, None, 6).unwrap()[0],
            [511, 0, -512].map(|i| tick_array(&key, i))
        );
    }

    #[test]
    fn activation_extensions_preserve_the_array_walks() {
        let key = Pubkey::new_unique();
        for rewards in [false, true] {
            let mut data = pool(0, 1, &[0, 1]);
            data[EXTENSIONS] = ACTIVATION_EXTENSION | if rewards { REWARD_EXTENSION } else { 0 };
            let offset = POOL_LEN + if rewards { REWARD_EXTENSION_LEN } else { 0 };
            data.resize(offset + ACTIVATION_EXTENSION_LEN, 0);
            assert_eq!(
                swap_walks(&key, &data, None, 2).unwrap(),
                [
                    vec![tick_array(&key, 0)],
                    vec![tick_array(&key, 0), tick_array(&key, 1)]
                ]
            );
            data.pop();
            assert_eq!(
                walk_error(&key, &data, None),
                format!("DLMM Pro pool {key} has a truncated activation extension")
            );
        }
    }

    /// A Token-2022 mint with the given extensions, as type, value.
    fn mint(extensions: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let mut data = vec![0; MINT_EXTENSIONS];
        data[MINT_ACCOUNT_TYPE] = MINT_ACCOUNT;
        for (kind, value) in extensions {
            data.extend(kind.to_le_bytes());
            data.extend((value.len() as u16).to_le_bytes());
            data.extend(value);
        }
        data
    }

    #[test]
    fn transfer_hooks_are_found_among_a_mints_extensions() {
        let hook = |program: Pubkey| {
            let mut value = Pubkey::new_unique().to_bytes().to_vec();
            value.extend(program.to_bytes());
            (TRANSFER_HOOK_EXTENSION, value)
        };
        let other = (1u16, vec![7; 108]);
        let cases = [
            ("base mint", vec![0; 82], false),
            ("no extensions", mint(&[]), false),
            ("other extension", mint(std::slice::from_ref(&other)), false),
            (
                "hook after another",
                mint(&[other.clone(), hook(Pubkey::new_unique())]),
                true,
            ),
            (
                "hook without a program",
                mint(&[hook(Pubkey::default())]),
                false,
            ),
        ];
        for (name, data, expected) in cases {
            assert_eq!(has_transfer_hook(&data).unwrap(), expected, "{name}");
        }
        let error = |data: &[u8]| has_transfer_hook(data).unwrap_err().to_string();
        let mut truncated = mint(&[other, hook(Pubkey::new_unique())]);
        truncated.pop();
        assert_eq!(error(&truncated), "truncated Token-2022 mint extension");
        let authority_only = (TRANSFER_HOOK_EXTENSION, vec![7; 32]);
        assert_eq!(
            error(&mint(&[authority_only])),
            "truncated transfer-hook extension"
        );
        let mut account = mint(&[]);
        account[MINT_ACCOUNT_TYPE] = 2;
        assert_eq!(error(&account), "not a Token-2022 mint account");
    }
}
