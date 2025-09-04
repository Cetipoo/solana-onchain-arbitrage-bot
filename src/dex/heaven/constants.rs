use solana_program::pubkey::Pubkey;
use std::str::FromStr;

pub const HEAVEN_PROGRAM_ID: &str = "HEAVENoP2qxoeuF8Dj2oT1GHEnu49U5mJYkdeC8BAX2o";
pub const HEAVEN_PROTOCOL_ACCOUNT_1: &str = "HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny";
pub const HEAVEN_PROTOCOL_ACCOUNT_2: &str = "CH31Xns5z3M1cTAbKW34jcxPPciazARpijcHj9rxtemt";

pub fn heaven_program_id() -> Pubkey {
    Pubkey::from_str(HEAVEN_PROGRAM_ID).unwrap()
}

pub fn heaven_protocol_account_1() -> Pubkey {
    Pubkey::from_str(HEAVEN_PROTOCOL_ACCOUNT_1).unwrap()
}

pub fn heaven_protocol_account_2() -> Pubkey {
    Pubkey::from_str(HEAVEN_PROTOCOL_ACCOUNT_2).unwrap()
}