pub mod info;

pub use info::*;

use solana_program::pubkey::Pubkey;
use std::str::FromStr;

pub fn futarchy_program_id() -> Pubkey {
    Pubkey::from_str("FUTARELBfJfQ8RDGhg1wdhddq1odMAJUePHFuBYfUxKq").unwrap()
}
