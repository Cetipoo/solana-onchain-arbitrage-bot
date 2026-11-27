//! Raydium CLMM pool state, which the PancakeSwap and Byreal forks share:
//! where its swap finds initialized tick arrays, and which pools the
//! executor cannot swap through.
use crate::v10 as abi;
use anyhow::{ensure, Result};
use solana_sdk::{account::Account, pubkey::Pubkey};

const POOL_DISCRIMINATOR: [u8; 8] = [0xf7, 0xed, 0xe3, 0xf5, 0xd7, 0xc3, 0xde, 0x46];
const POOL_LEN: usize = 1544;
const EXTENSION_DISCRIMINATOR: [u8; 8] = [0x3c, 0x96, 0x24, 0xdb, 0x61, 0x80, 0x8b, 0x99];
const EXTENSION_LEN: usize = 1832;
const TICKS_PER_ARRAY: i32 = 60;
const MIN_TICK: i32 = -443_636;
const MAX_TICK: i32 = 443_636;
/// Arrays one extension bitmap marks; the pool's own bitmap marks twice as
/// many, the arrays at indices `[-512, 512)`.
const ARRAYS_PER_BITMAP: i32 = 512;
/// Bitmaps the extension holds on each side of the pool's own.
const EXTENSION_BITMAPS: usize = 14;
/// The `status` bit that disables swaps.
const SWAP_DISABLED: u8 = 1 << 4;

/// `N` little-endian words from `offset`.
fn words<const N: usize>(data: &[u8], offset: usize) -> Result<[u64; N]> {
    let mut words = [0; N];
    for (index, word) in words.iter_mut().enumerate() {
        *word = u64::from_le_bytes(abi::read(data, offset + 8 * index)?);
    }
    Ok(words)
}

pub(crate) struct Pool {
    address: Pubkey,
    bump: u8,
    amm_config: Pubkey,
    mints: [Pubkey; 2],
    tick_spacing: i32,
    tick_current: i32,
    status: u8,
    seed_index: [u8; 2],
    bitmap: [u64; 16],
    open_time: u64,
}

impl Pool {
    /// A pool with a zero spacing is invalid: `array_index` divides by it.
    /// Any tick decodes.
    pub(crate) fn decode(pool: Pubkey, data: &[u8]) -> Result<Self> {
        ensure!(
            data.len() >= POOL_LEN && data[..8] == POOL_DISCRIMINATOR,
            "invalid CLMM pool {pool}"
        );
        let tick_spacing = i32::from(u16::from_le_bytes(abi::read(data, 235)?));
        ensure!(
            tick_spacing != 0,
            "CLMM pool {pool} has a zero tick spacing"
        );
        Ok(Self {
            address: pool,
            bump: data[8],
            amm_config: Pubkey::new_from_array(abi::read(data, 9)?),
            mints: abi::PoolPair::read(abi::CLMM, data)?.mints,
            tick_spacing,
            tick_current: i32::from_le_bytes(abi::read(data, 269)?),
            status: data[389],
            seed_index: abi::read(data, 391)?,
            bitmap: words(data, 904)?,
            open_time: u64::from_le_bytes(abi::read(data, 1080)?),
        })
    }

    fn array_ticks(&self) -> i32 {
        TICKS_PER_ARRAY * self.tick_spacing
    }

    /// The index of the array holding `tick`, its start over `array_ticks`.
    fn array_index(&self, tick: i32) -> i32 {
        tick.div_euclid(self.array_ticks())
    }

    /// The bitmap marking the array at `index`: the pool's own for
    /// `[-512, 512)`, else, for an array holding a tick of the range, the
    /// `extension`'s. The pool's own bitmap covers the range from spacing 15
    /// on.
    fn bitmap<'a>(&'a self, extension: Option<&'a Extension>, index: i32) -> Option<Bitmap<'a>> {
        if (-ARRAYS_PER_BITMAP..ARRAYS_PER_BITMAP).contains(&index) {
            return Some(Bitmap {
                words: &self.bitmap,
                first: -ARRAYS_PER_BITMAP,
            });
        }
        let in_range = self.array_index(MIN_TICK)..=self.array_index(MAX_TICK);
        if !in_range.contains(&index) {
            return None;
        }
        extension?.bitmap(index)
    }

    /// The start of each initialized array a `zero_for_one` swap enters, in
    /// order: the current tick's if its bit is set, then the nearest marked
    /// bit of each bitmap in turn. The walk ends where it needs a bitmap past
    /// the tick range or in an `extension` the pool does not have. Each
    /// bitmap is scanned whole, so the last start may be of an array past the
    /// range's last one.
    pub(crate) fn initialized_starts<'a>(
        &'a self,
        extension: Option<&'a Extension>,
        zero_for_one: bool,
    ) -> impl Iterator<Item = i32> + 'a {
        let step = if zero_for_one { -1 } else { 1 };
        let marked_past = move |index: i32| -> Option<i32> {
            let mut index = index + step;
            loop {
                let bitmap = self.bitmap(extension, index)?;
                if let Some(marked) = bitmap.nearest_marked(index, zero_for_one) {
                    return Some(marked);
                }
                index = bitmap.beyond(zero_for_one);
            }
        };
        // Raydium looks past an unset current array from the next index, not
        // from the current one. Below spacing 15 the range's last array lies
        // in an extension bitmap, and a walk from it ends even where that
        // bitmap marks arrays past the range; from spacing 15 it lies in the
        // pool's own bitmap, which the walk scans to its end.
        let current = self.array_index(self.tick_current);
        let first = self.bitmap(extension, current).and_then(|bitmap| {
            if bitmap.is_set(current) {
                Some(current)
            } else {
                marked_past(current)
            }
        });
        std::iter::successors(first, move |&index| marked_past(index))
            .map(move |index| index * self.array_ticks())
    }

    /// Whether the pool sits at the Raydium pool address its state derives
    /// under `program`.
    fn is_at_derived_address(&self, program: Pubkey) -> bool {
        // Permissioned pools add their nonzero little-endian `seed_index`
        // after the mints; legacy pools keep their original address by
        // hashing an empty seed there, as the program does.
        let seed_index: &[u8] = if self.seed_index == [0, 0] {
            &[]
        } else {
            &self.seed_index
        };
        Pubkey::create_program_address(
            &[
                b"pool",
                self.amm_config.as_ref(),
                self.mints[0].as_ref(),
                self.mints[1].as_ref(),
                seed_index,
                &[self.bump],
            ],
            &program,
        )
        .is_ok_and(|derived| derived == self.address)
    }

    /// Fails where the executor cannot swap through this pool under
    /// `program` at `unix_timestamp`.
    pub(crate) fn ensure_swappable(&self, program: Pubkey, unix_timestamp: i64) -> Result<()> {
        let address = self.address;
        ensure!(
            program != abi::CLMM || self.is_at_derived_address(program),
            "CLMM pool {address} is not at its Raydium pool address"
        );
        ensure!(
            self.status & SWAP_DISABLED == 0,
            "CLMM pool {address} has swaps disabled"
        );
        // Every fork swaps only once the clock is past `open_time`.
        ensure!(
            u64::try_from(unix_timestamp).is_ok_and(|now| now > self.open_time),
            "CLMM pool {address} is not open for swaps"
        );
        Ok(())
    }
}

/// A pool's tick array bitmap extension: the bitmaps beyond the pool's own,
/// which a pool with a spacing below 15 needs.
pub(crate) struct Extension {
    positive: [[u64; 8]; EXTENSION_BITMAPS],
    negative: [[u64; 8]; EXTENSION_BITMAPS],
}

impl Extension {
    /// The extension in `account`, or `None` where the account holds no
    /// data. Fails where the data is not `pool`'s extension under `program`.
    pub(crate) fn decode(pool: Pubkey, program: Pubkey, account: &Account) -> Result<Option<Self>> {
        let data = &account.data;
        if data.is_empty() {
            return Ok(None);
        }
        ensure!(
            account.owner == program
                && data.len() >= EXTENSION_LEN
                && data[..8] == EXTENSION_DISCRIMINATOR
                && data[8..40] == pool.to_bytes(),
            "invalid CLMM bitmap extension for {pool}"
        );
        // Each 512-bit bitmap is 8 words, the positive ones first.
        let bitmaps = |first: usize| -> Result<_> {
            let mut bitmaps = [[0; 8]; EXTENSION_BITMAPS];
            for (index, bitmap) in bitmaps.iter_mut().enumerate() {
                *bitmap = words(data, 40 + 64 * (first + index))?;
            }
            Ok(bitmaps)
        };
        Ok(Some(Self {
            positive: bitmaps(0)?,
            negative: bitmaps(EXTENSION_BITMAPS)?,
        }))
    }

    /// The bitmap marking the array at `index`, an index outside the pool's
    /// own bitmap. `positive[k]` marks `[(k + 1) * 512, (k + 2) * 512)` and
    /// `negative[k]` `[-(k + 2) * 512, -(k + 1) * 512)`.
    fn bitmap(&self, index: i32) -> Option<Bitmap<'_>> {
        let (bitmaps, k, first) = if index >= ARRAYS_PER_BITMAP {
            let k = index / ARRAYS_PER_BITMAP - 1;
            (&self.positive, k, (k + 1) * ARRAYS_PER_BITMAP)
        } else {
            let k = (-index - 1) / ARRAYS_PER_BITMAP - 1;
            (&self.negative, k, -(k + 2) * ARRAYS_PER_BITMAP)
        };
        Some(Bitmap {
            words: bitmaps.get(usize::try_from(k).ok()?)?,
            first,
        })
    }
}

/// Bitmap `words` marking consecutive arrays from index `first`, one bit
/// each. Its methods take an array index the bitmap marks, as
/// `Pool::bitmap` returns the bitmap marking the index it is given.
struct Bitmap<'a> {
    words: &'a [u64],
    first: i32,
}

impl Bitmap<'_> {
    /// The word and bit marking the array at `index`.
    fn position(&self, index: i32) -> (usize, u32) {
        let bit = (index - self.first) as usize;
        (bit / 64, (bit % 64) as u32)
    }

    fn is_set(&self, index: i32) -> bool {
        let (word, bit) = self.position(index);
        self.words[word] >> bit & 1 == 1
    }

    /// The marked array index nearest `index`: at or below it when `down`,
    /// else at or above it.
    fn nearest_marked(&self, index: i32, down: bool) -> Option<i32> {
        let (word, bit) = self.position(index);
        let at_or_past_index = if down {
            u64::MAX >> (63 - bit)
        } else {
            u64::MAX << bit
        };
        let marked = |at: usize| {
            let marks = if at == word {
                self.words[at] & at_or_past_index
            } else {
                self.words[at]
            };
            (marks != 0).then(|| {
                let bit = if down {
                    63 - marks.leading_zeros()
                } else {
                    marks.trailing_zeros()
                };
                self.first + (at * 64) as i32 + bit as i32
            })
        };
        if down {
            (0..=word).rev().find_map(marked)
        } else {
            (word..self.words.len()).find_map(marked)
        }
    }

    /// The first array index past this bitmap in a `down` or up walk.
    fn beyond(&self, down: bool) -> i32 {
        if down {
            self.first - 1
        } else {
            self.first + 64 * self.words.len() as i32
        }
    }
}
