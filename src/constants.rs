use solana_program::pubkey::Pubkey;

#[inline(always)]
pub fn sol_mint() -> Pubkey {
    spl_token::native_mint::id()
}
