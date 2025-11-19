use solana_program::pubkey::Pubkey;
use std::str::FromStr;

pub const PUMP_PROGRAM_ID: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";
pub const PUMP_FEE_WALLET: &str = "JCRGumoE9Qi5BBgULTgdgTLjSgkCMSbF62ZZfGs84JeU";
pub const PUMP_MAYHEM_FEE_WALLET: &str = "GesfTA3X2arioaHp8bbKdjG9vJtskViWACZoYvxp4twS";

pub fn pump_program_id() -> Pubkey {
    Pubkey::from_str(PUMP_PROGRAM_ID).unwrap()
}

pub fn pump_fee_wallet() -> Pubkey {
    Pubkey::from_str(PUMP_FEE_WALLET).unwrap()
}

pub fn pump_mayhem_fee_wallet() -> Pubkey {
    Pubkey::from_str(PUMP_MAYHEM_FEE_WALLET).unwrap()
}
