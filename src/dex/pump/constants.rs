use solana_program::{pubkey, pubkey::Pubkey};

const PUMP_PROGRAM_ID: Pubkey = pubkey!("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
const PUMP_FEE_WALLET: Pubkey = pubkey!("JCRGumoE9Qi5BBgULTgdgTLjSgkCMSbF62ZZfGs84JeU");

#[inline(always)]
pub fn pump_program_id() -> Pubkey {
    PUMP_PROGRAM_ID
}

#[inline(always)]
pub fn pump_fee_wallet() -> Pubkey {
    PUMP_FEE_WALLET
}
