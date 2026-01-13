use anyhow::Result;
use solana_program::pubkey::Pubkey;

// Byte offsets for Futarchy DAO account parsing
// Reference: ~/solana/arb-bot-rust/programs/executor-pinocchio/src/futarchy.rs
const BASE_MINT_OFFSET: usize = 157;
const QUOTE_MINT_OFFSET: usize = 189;
const BASE_VAULT_OFFSET: usize = 221;
const QUOTE_VAULT_OFFSET: usize = 253;

pub struct FutarchyInfo {
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub base_vault: Pubkey,
    pub quote_vault: Pubkey,
}

impl FutarchyInfo {
    pub fn load_checked(data: &[u8]) -> Result<Self> {
        if data.len() < QUOTE_VAULT_OFFSET + 32 {
            return Err(anyhow::anyhow!("Invalid data length for FutarchyInfo"));
        }

        let base_mint = read_pubkey(data, BASE_MINT_OFFSET)?;
        let quote_mint = read_pubkey(data, QUOTE_MINT_OFFSET)?;
        let base_vault = read_pubkey(data, BASE_VAULT_OFFSET)?;
        let quote_vault = read_pubkey(data, QUOTE_VAULT_OFFSET)?;

        Ok(Self {
            base_mint,
            quote_mint,
            base_vault,
            quote_vault,
        })
    }
}

fn read_pubkey(data: &[u8], offset: usize) -> Result<Pubkey> {
    if data.len() < offset + 32 {
        return Err(anyhow::anyhow!("Data too short to read pubkey at offset {}", offset));
    }
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&data[offset..offset + 32]);
    Ok(Pubkey::new_from_array(bytes))
}
