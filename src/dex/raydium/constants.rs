use solana_program::{pubkey, pubkey::Pubkey};

const RAYDIUM_AMM_PROGRAM_ID: Pubkey = pubkey!("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");
const RAYDIUM_AMM_AUTHORITY: Pubkey = pubkey!("5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1");

const RAYDIUM_CPMM_PROGRAM_ID: Pubkey = pubkey!("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C");
const RAYDIUM_CPMM_AUTHORITY: Pubkey = pubkey!("GpMZbSM2GgvTKHJirzeGfMFoaZ8UR2X7F4v8vHTvxFbL");

const RAYDIUM_CLMM_PROGRAM_ID: Pubkey = pubkey!("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");

#[inline(always)]
pub fn raydium_program_id() -> Pubkey {
    RAYDIUM_AMM_PROGRAM_ID
}

#[inline(always)]
pub fn raydium_authority() -> Pubkey {
    RAYDIUM_AMM_AUTHORITY
}

#[inline(always)]
pub fn raydium_cp_program_id() -> Pubkey {
    RAYDIUM_CPMM_PROGRAM_ID
}

#[inline(always)]
pub fn raydium_cp_authority() -> Pubkey {
    RAYDIUM_CPMM_AUTHORITY
}

#[inline(always)]
pub fn raydium_clmm_program_id() -> Pubkey {
    RAYDIUM_CLMM_PROGRAM_ID
}
