//! Archer markets as V10 pools: a market and the maker books its swap
//! aggregator fills from. Market state does not name its books; a loader
//! finds them with a program scan for accounts of `BOOK_LEN` bytes holding
//! the market at `BOOK_MARKET`.
use crate::v10::read;
use anyhow::{ensure, Result};
use executor_v10_abi::{
    cost::{ARCHER_BOOK_CU, ARCHER_BOOK_PARSE_CU},
    ARCHER_MAX_BOOKS, ARCHER_POOL_ACCOUNTS,
};
use solana_sdk::{instruction::AccountMeta, pubkey, pubkey::Pubkey};

pub const ARCHER: Pubkey = pubkey!("Archer8kgiavM61GyusMzaaS2ft5sALtNsD1HxkUPMhy");

const MARKET_LEN: usize = 272;
const MARKET_DISCRIMINATOR: &[u8; 8] = b"ACHRMKT1";
/// The market's base and quote mints, then its base and quote vaults.
pub(crate) const MARKET_PAIR: (usize, usize, usize, usize) = (40, 72, 104, 136);
const MARKET_QUOTE_SCALE_DENOMINATOR: usize = 224;
/// The base lot, quote lot, price scale and quote scale denominator, each a
/// u64 the executor rejects a whole transaction for when zero.
const MARKET_UNITS: [usize; 4] = [200, 208, 216, MARKET_QUOTE_SCALE_DENOMINATOR];
const MARKET_TAKER_FEE: usize = 252;
const MARKET_BASE_DECIMALS: usize = 256;
const MARKET_STATUS: usize = 258;
const MARKET_ACTIVE: u8 = 0;

pub const BOOK_LEN: usize = 776;
const BOOK_DISCRIMINATOR: &[u8; 8] = b"ACHRMKR1";
const BOOK_AUTHORITY: usize = 8;
/// The market a book quotes.
pub const BOOK_MARKET: usize = 40;
const BOOK_DELEGATE: usize = 72;
const BOOK_MID: usize = 104;
const BOOK_STATUS: usize = 160;
const BOOK_LAST_UPDATE: usize = 712;
const BOOK_EXPIRY: usize = 720;
const BOOK_ACTIVE: u8 = 1;

/// Whether `data` is an Archer market's account.
pub(crate) fn is_market(data: &[u8]) -> bool {
    data.len() == MARKET_LEN && data.starts_with(MARKET_DISCRIMINATOR)
}

/// Checks that the executor can quote `market`: it is active, its lot sizes
/// and scales are nonzero, and its taker fee and base decimals are in range.
/// Otherwise the executor rejects the transaction or quotes no fill.
pub fn check_market(market: &Pubkey, data: &[u8]) -> Result<()> {
    ensure!(is_market(data), "{market} is not an Archer market");
    ensure!(
        data[MARKET_STATUS] == MARKET_ACTIVE,
        "Archer market {market} is not active"
    );
    for offset in MARKET_UNITS {
        ensure!(
            u64::from_le_bytes(read(data, offset)?) != 0,
            "Archer market {market} has a zero lot size or scale"
        );
    }
    ensure!(
        i32::from_le_bytes(read(data, MARKET_TAKER_FEE)?) >= 0,
        "Archer market {market} has a negative taker fee"
    );
    let quote_scale = u64::from_le_bytes(read(data, MARKET_QUOTE_SCALE_DENOMINATOR)?);
    ensure!(
        10u128
            .checked_pow(u32::from(data[MARKET_BASE_DECIMALS]))
            .and_then(|scale| scale.checked_mul(u128::from(quote_scale)))
            .is_some(),
        "Archer market {market}'s price denominator overflows"
    );
    Ok(())
}

/// A maker book of a market, as a program scan returns it.
#[derive(Clone, Copy, Debug)]
pub struct Book<'a> {
    pub key: Pubkey,
    pub data: &'a [u8],
}

/// The books a block of `market` lists for `wallet` at `slot`, after the
/// cluster's `last_restart_slot`, from `books` with distinct keys as a
/// program scan returns them: those passing the aggregator's status, wallet,
/// restart, expiry and mid checks, the most recently updated first up to
/// `ARCHER_MAX_BOOKS`, in address order. A suspended or expired book, or one
/// the wallet makes markets with, costs work and never fills; a listed book
/// of another status rejects the swap. A deferred bid reserve is not
/// projected, so a book the aggregator skips for one is listed and fills
/// nothing. Accounts that are not a book of `market` are ignored.
pub fn select_books<'a>(
    market: &Pubkey,
    wallet: &Pubkey,
    slot: u64,
    last_restart_slot: u64,
    books: impl IntoIterator<Item = Book<'a>>,
) -> Result<Vec<Pubkey>> {
    let mut admitted = Vec::new();
    for book in books {
        let data = book.data;
        let key_at = |offset: usize| data[offset..offset + 32] == wallet.to_bytes();
        if data.len() != BOOK_LEN
            || !data.starts_with(BOOK_DISCRIMINATOR)
            || data[BOOK_MARKET..BOOK_MARKET + 32] != market.to_bytes()
            || data[BOOK_STATUS] != BOOK_ACTIVE
            || key_at(BOOK_AUTHORITY)
            || key_at(BOOK_DELEGATE)
        {
            continue;
        }
        let last_update = u64::from_le_bytes(read(data, BOOK_LAST_UPDATE)?);
        let expiry = u64::from_le_bytes(read(data, BOOK_EXPIRY)?);
        // An expiry of zero never expires; the aggregator skips a book whose
        // mid does not fit i64.
        if last_update <= last_restart_slot
            || slot.saturating_sub(last_update) > expiry.wrapping_sub(1)
            || i64::try_from(u64::from_le_bytes(read(data, BOOK_MID)?)).is_err()
        {
            continue;
        }
        admitted.push((last_update, book.key));
    }
    admitted.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    admitted.truncate(ARCHER_MAX_BOOKS);
    let mut keys: Vec<_> = admitted.into_iter().map(|(_, key)| key).collect();
    keys.sort_unstable();
    ensure!(
        !keys.is_empty(),
        "Archer market {market} has no book the aggregator admits"
    );
    Ok(keys)
}

/// The maker books a pool `block`, its program first, lists: none unless it
/// is an Archer block.
pub(crate) fn book_count(block: &[AccountMeta]) -> u32 {
    if block.first().is_some_and(|meta| meta.pubkey == ARCHER) {
        block.len().saturating_sub(ARCHER_POOL_ACCOUNTS) as u32
    } else {
        0
    }
}

/// The executor's work for a swap through `block` beyond its venue's.
pub(crate) fn swap_cu(block: &[AccountMeta]) -> u32 {
    book_count(block) * ARCHER_BOOK_CU
}

/// The executor's work admitting the books of every pool in `blocks`, which
/// it does for each listed pool whether or not a route trades it.
pub(crate) fn parse_cu<'a>(blocks: impl IntoIterator<Item = &'a [AccountMeta]>) -> u32 {
    blocks.into_iter().map(book_count).sum::<u32>() * ARCHER_BOOK_PARSE_CU
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOT: u64 = 1_000;
    const RESTART: u64 = 500;

    struct Spec {
        market: Pubkey,
        authority: Pubkey,
        delegate: Pubkey,
        status: u8,
        last_update: u64,
        expiry: u64,
        mid: u64,
    }

    impl Spec {
        fn active(market: Pubkey) -> Self {
            Self {
                market,
                authority: Pubkey::new_unique(),
                delegate: Pubkey::default(),
                status: 1,
                last_update: SLOT - 10,
                expiry: 0,
                mid: 1,
            }
        }

        /// A book in the layout `select_books` reads, written from literal
        /// offsets rather than the module's constants.
        fn data(&self) -> Vec<u8> {
            let mut data = vec![0; 776];
            data[..8].copy_from_slice(b"ACHRMKR1");
            for (offset, key) in [(8, self.authority), (40, self.market), (72, self.delegate)] {
                data[offset..offset + 32].copy_from_slice(key.as_ref());
            }
            data[160] = self.status;
            for (offset, value) in [(104, self.mid), (712, self.last_update), (720, self.expiry)] {
                data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
            }
            data
        }
    }

    fn select(
        market: &Pubkey,
        wallet: &Pubkey,
        books: &[(Pubkey, Vec<u8>)],
    ) -> Result<Vec<Pubkey>> {
        select_books(
            market,
            wallet,
            SLOT,
            RESTART,
            books.iter().map(|(key, data)| Book { key: *key, data }),
        )
    }

    /// Each book alone, and whether the aggregator admits it.
    #[test]
    fn books_are_listed_only_when_the_aggregator_admits_them() {
        let market = Pubkey::new_unique();
        let wallet = Pubkey::new_unique();
        let edit = |change: &dyn Fn(&mut Spec)| {
            let mut spec = Spec::active(market);
            change(&mut spec);
            spec.data()
        };
        let cases: Vec<(&str, Vec<u8>, bool)> = vec![
            ("active without expiry", edit(&|_| {}), true),
            ("suspended", edit(&|s| s.status = 2), false),
            ("other status", edit(&|s| s.status = 3), false),
            ("uninitialized status", edit(&|s| s.status = 0), false),
            ("wallet authority", edit(&|s| s.authority = wallet), false),
            ("wallet delegate", edit(&|s| s.delegate = wallet), false),
            (
                "other market",
                edit(&|s| s.market = Pubkey::new_unique()),
                false,
            ),
            (
                "updated before restart",
                edit(&|s| s.last_update = RESTART - 1),
                false,
            ),
            (
                "updated at restart",
                edit(&|s| s.last_update = RESTART),
                false,
            ),
            (
                "updated just after restart",
                edit(&|s| s.last_update = RESTART + 1),
                true,
            ),
            // Age 10 against expiry 11, 10 and 1.
            ("younger than expiry", edit(&|s| s.expiry = 11), true),
            ("aged to expiry", edit(&|s| s.expiry = 10), false),
            ("expiry one", edit(&|s| s.expiry = 1), false),
            (
                "updated this slot, expiry one",
                edit(&|s| {
                    s.last_update = SLOT;
                    s.expiry = 1;
                }),
                true,
            ),
            (
                "updated after this slot",
                edit(&|s| {
                    s.last_update = SLOT + 5;
                    s.expiry = 1;
                }),
                true,
            ),
            ("mid at i64::MAX", edit(&|s| s.mid = i64::MAX as u64), true),
            (
                "mid above i64::MAX",
                edit(&|s| s.mid = i64::MAX as u64 + 1),
                false,
            ),
            ("truncated", edit(&|_| {})[..775].to_vec(), false),
            (
                "other discriminator",
                {
                    let mut data = edit(&|_| {});
                    data[0] ^= 1;
                    data
                },
                false,
            ),
        ];
        let mut admitted = 0;
        let mut skipped = 0;
        for (name, data, expected) in &cases {
            let key = Pubkey::new_unique();
            let result = select(&market, &wallet, &[(key, data.clone())]);
            if *expected {
                assert_eq!(result.unwrap(), vec![key], "{name}");
                admitted += 1;
            } else {
                assert_eq!(
                    result.unwrap_err().to_string(),
                    format!("Archer market {market} has no book the aggregator admits"),
                    "{name}"
                );
                skipped += 1;
            }
        }
        assert_eq!((admitted, skipped), (6, 13));
    }

    #[test]
    fn the_most_recent_books_are_listed_in_address_order() {
        let market = Pubkey::new_unique();
        let wallet = Pubkey::new_unique();
        let book = |last_update: u64| {
            let mut spec = Spec::active(market);
            spec.last_update = last_update;
            (Pubkey::new_unique(), spec.data())
        };
        // 25 admitted books updated at slots 900..925, then a suspended one.
        let mut books: Vec<_> = (900..925).map(book).collect();
        let mut suspended = Spec::active(market);
        suspended.status = 2;
        suspended.last_update = SLOT;
        books.push((Pubkey::new_unique(), suspended.data()));
        let mut expected: Vec<_> = books[6..25].iter().map(|(key, _)| *key).collect();
        expected.sort_unstable();
        assert_eq!(select(&market, &wallet, &books).unwrap(), expected);
        assert_eq!(expected.len(), ARCHER_MAX_BOOKS);

        // Books updated in the same slot at the cut keep the lower addresses.
        let mut tied: Vec<_> = (0..ARCHER_MAX_BOOKS + 2).map(|_| book(950)).collect();
        tied.sort_unstable_by_key(|(key, _)| *key);
        let expected: Vec<_> = tied[..ARCHER_MAX_BOOKS]
            .iter()
            .map(|(key, _)| *key)
            .collect();
        tied.reverse();
        assert_eq!(select(&market, &wallet, &tied).unwrap(), expected);
    }

    #[test]
    fn block_book_counts_and_cu_follow_the_program() {
        let meta = |key| AccountMeta::new(key, false);
        let mut block = vec![meta(ARCHER)];
        block.extend((0..4).map(|_| meta(Pubkey::new_unique())));
        assert_eq!(book_count(&block), 0);
        block.extend((0..7).map(|_| meta(Pubkey::new_unique())));
        assert_eq!(book_count(&block), 7);
        assert_eq!(swap_cu(&block), 7 * ARCHER_BOOK_CU);
        assert_eq!(parse_cu([&block[..]]), 7 * ARCHER_BOOK_PARSE_CU);
        let archer = block.clone();
        block[0] = meta(crate::v10::MANIFEST);
        assert_eq!(
            (book_count(&block), swap_cu(&block), parse_cu([&block[..]])),
            (0, 0, 0)
        );
        assert_eq!(
            parse_cu([&archer[..], &block[..], &archer[..]]),
            14 * ARCHER_BOOK_PARSE_CU
        );
    }
}
