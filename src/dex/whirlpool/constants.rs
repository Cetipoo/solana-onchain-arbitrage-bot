use solana_program::{pubkey, pubkey::Pubkey};

const WHIRLPOOL_PROGRAM_ID: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");

pub const MAX_TICK_INDEX: i32 = 443636;
pub const MIN_TICK_INDEX: i32 = -443636;

#[inline(always)]
pub fn whirlpool_program_id() -> Pubkey {
    WHIRLPOOL_PROGRAM_ID
}
