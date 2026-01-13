use solana_program::pubkey::Pubkey;
use std::str::FromStr;

pub fn pancakeswap_program_id() -> Pubkey {
    Pubkey::from_str("HpNfyc2Saw7RKkQd8nEL4khUcuPhQ7WwY1B2qjx8jxFq").unwrap()
}

pub fn pancakeswap_authority() -> Pubkey {
    Pubkey::from_str("GThUX1Atko4tqhN2NaiTazWSeFWMuiUvfFnyJyUghFMJ").unwrap()
}
