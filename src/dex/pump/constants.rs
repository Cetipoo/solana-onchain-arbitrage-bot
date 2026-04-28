use solana_program::pubkey::Pubkey;
use std::str::FromStr;

pub const PUMP_PROGRAM_ID: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";
pub const PUMP_FEE_WALLETS: [&str; 8] = [
    "62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV",
    "7VtfL8fvgNfhz17qKRMjzQEXgbdpnHHHQRh54R9jP2RJ",
    "7hTckgnGnLQR6sdH7YkqFTAA7VwTfYFaZ6EhEsU3saCX",
    "9rPYyANsfQZw3DnDmKE3YCQF5E8oD89UXoHn9JFEhJUz",
    "AVmoTthdrX6tKt4nDjco2D775W2YK3sDhxPcMmzUAmTY",
    "FWsW1xNtWscwNmKv6wVsU1iTzRN6wmmk3MjxRP5tT7hz",
    "G5UZAVbAf46s7cKWoyKu8kYTip9DGTpbLZ2qa9Aq69dP",
    "JCRGumoE9Qi5BBgULTgdgTLjSgkCMSbF62ZZfGs84JeU",
];
pub const PUMP_MAYHEM_FEE_WALLETS: [&str; 8] = [
    "GesfTA3X2arioaHp8bbKdjG9vJtskViWACZoYvxp4twS",
    "4budycTjhs9fD6xw62VBducVTNgMgJJ5BgtKq7mAZwn6",
    "8SBKzEQU4nLSzcwF4a74F2iaUDQyTfjGndn6qUWBnrpR",
    "4UQeTP1T39KZ9Sfxzo3WR5skgsaP6NZa87BAkuazLEKH",
    "8sNeir4QsLsJdYpc9RZacohhK1Y5FLU3nC5LXgYB4aa6",
    "Fh9HmeLNUMVCvejxCtCL2DbYaRyBFVJ5xrWkLnMH6fdk",
    "463MEnMeGyJekNZFQSTUABBEbLnvMTALbT6ZmsxAbAdq",
    "6AUH3WEHucYZyC61hqpqYUWVto5qA5hjHuNQ32GNnNxA",
];
pub const PUMP_SWAP_FEE_RECIPIENT: &str = "EHAAiTxcdDwQ3U4bU6YcMsQGaekdzLS3B5SmYo46kJtL";

pub fn pump_program_id() -> Pubkey {
    Pubkey::from_str(PUMP_PROGRAM_ID).unwrap()
}

pub fn pump_fee_wallet() -> Pubkey {
    let idx = rand::random::<usize>() % PUMP_FEE_WALLETS.len();
    Pubkey::from_str(PUMP_FEE_WALLETS[idx]).unwrap()
}

pub fn pump_mayhem_fee_wallet() -> Pubkey {
    let idx = rand::random::<usize>() % PUMP_MAYHEM_FEE_WALLETS.len();
    Pubkey::from_str(PUMP_MAYHEM_FEE_WALLETS[idx]).unwrap()
}

pub fn pump_swap_fee_recipient() -> Pubkey {
    Pubkey::from_str(PUMP_SWAP_FEE_RECIPIENT).unwrap()
}
