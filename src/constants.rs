use solana_program::pubkey::Pubkey;
use std::str::FromStr;

pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
pub const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
pub const USD1_MINT: &str = "USD1ttGY1N17NEEHLmELoaybftRBUSErhqYiQzvEmuB";

pub fn sol_mint() -> Pubkey {
    Pubkey::from_str(SOL_MINT).unwrap()
}

pub fn usdc_mint() -> Pubkey {
    Pubkey::from_str(USDC_MINT).unwrap()
}

pub fn usd1_mint() -> Pubkey {
    Pubkey::from_str(USD1_MINT).unwrap()
}
