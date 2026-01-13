pub mod info;

pub use info::*;

use solana_program::pubkey::Pubkey;
use std::str::FromStr;

pub fn humidifi_program_id() -> Pubkey {
    Pubkey::from_str("9H6tua7jkLhdm3w8BvgpTn5LZNU7g4ZynDmCiNN3q6Rp").unwrap()
}
