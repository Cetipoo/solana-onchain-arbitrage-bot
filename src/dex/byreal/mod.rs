use solana_program::pubkey::Pubkey;
use std::str::FromStr;

pub fn byreal_program_id() -> Pubkey {
    Pubkey::from_str("REALQqNEomY6cQGZJUGwywTBD2UmDT32rZcNnfxQ5N2").unwrap()
}

pub fn byreal_authority() -> Pubkey {
    Pubkey::from_str("GThUX1Atko4tqhN2NaiTazWSeFWMuiUvfFnyJyUghFMJ").unwrap()
}
