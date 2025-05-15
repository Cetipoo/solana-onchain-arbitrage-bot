use solana_program::{pubkey, pubkey::Pubkey};

const METEORA_DLMM_PROGRAM_ID: Pubkey = pubkey!("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");
const METEORA_DLMM_EVENT_AUTHORITY: Pubkey =
    pubkey!("D1ZN9Wj1fRSUQfCjhvnu1hqDMT7hzjzBBpi12nVniYD6");

const METEORA_DAMM_PROGRAM_ID: Pubkey = pubkey!("Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB");
const METEORA_VAULT_PROGRAM_ID: Pubkey = pubkey!("24Uqj9JCLxUeoC3hGfh5W3s9FM9uCHDS2SG3LYwBpyTi");

#[inline(always)]
pub fn dlmm_program_id() -> Pubkey {
    METEORA_DLMM_PROGRAM_ID
}

#[inline(always)]
pub fn dlmm_event_authority() -> Pubkey {
    METEORA_DLMM_EVENT_AUTHORITY
}

#[inline(always)]
pub fn damm_program_id() -> Pubkey {
    METEORA_DAMM_PROGRAM_ID
}

#[inline(always)]
pub fn vault_program_id() -> Pubkey {
    METEORA_VAULT_PROGRAM_ID
}

pub const BIN_ARRAY: &[u8] = b"bin_array";
