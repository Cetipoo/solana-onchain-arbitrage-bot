use anyhow::Result;
use solana_program::pubkey::Pubkey;

// Reference: ~/solana/arb-bot-rust/lib/dex-humidifi/src/pool_decoder.rs

// XOR keys for decoding pubkeys stored in pool data.
// Each 32-byte pubkey is split into 4 x 8-byte chunks, each XOR'd with corresponding key.
const XOR_KEYS: [u64; 4] = [
    0xfb5c_e87a_ae44_3c38,
    0x04a2_1784_51ba_c3c7,
    0x04a1_1787_51b9_c3c6,
    0x04a0_1786_51b8_c3c5,
];

// Offsets for XOR-encoded pubkeys
const QUOTE_MINT_OFFSET: usize = 0x180;
const BASE_MINT_OFFSET: usize = 0x1a0;
const QUOTE_VAULT_OFFSET: usize = 0x1c0;
const BASE_VAULT_OFFSET: usize = 0x1e0;

pub struct HumidifiInfo {
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub base_vault: Pubkey,
    pub quote_vault: Pubkey,
}

impl HumidifiInfo {
    pub fn load_checked(data: &[u8]) -> Result<Self> {
        if data.len() < BASE_VAULT_OFFSET + 32 {
            return Err(anyhow::anyhow!("Invalid data length for HumidifiInfo"));
        }

        let quote_mint = decode_pubkey(data, QUOTE_MINT_OFFSET)
            .ok_or_else(|| anyhow::anyhow!("Failed to decode quote_mint"))?;
        let base_mint = decode_pubkey(data, BASE_MINT_OFFSET)
            .ok_or_else(|| anyhow::anyhow!("Failed to decode base_mint"))?;
        let quote_vault = decode_pubkey(data, QUOTE_VAULT_OFFSET)
            .ok_or_else(|| anyhow::anyhow!("Failed to decode quote_vault"))?;
        let base_vault = decode_pubkey(data, BASE_VAULT_OFFSET)
            .ok_or_else(|| anyhow::anyhow!("Failed to decode base_vault"))?;

        Ok(Self {
            base_mint,
            quote_mint,
            base_vault,
            quote_vault,
        })
    }
}

/// Decode a 32-byte pubkey from XOR-encoded pool data.
/// Each pubkey is stored as 4 consecutive u64 values, each XOR'd with a key.
fn decode_pubkey(pool_data: &[u8], offset: usize) -> Option<Pubkey> {
    if pool_data.len() < offset + 32 {
        return None;
    }
    let mut result = [0u8; 32];
    for i in 0..4 {
        let chunk_offset = offset + (i * 8);
        let chunk = read_u64_le(pool_data, chunk_offset)?;
        let decoded = chunk ^ XOR_KEYS[i];
        result[i * 8..(i + 1) * 8].copy_from_slice(&decoded.to_le_bytes());
    }
    Some(Pubkey::new_from_array(result))
}

fn read_u64_le(data: &[u8], offset: usize) -> Option<u64> {
    if data.len() < offset + 8 {
        return None;
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&data[offset..offset + 8]);
    Some(u64::from_le_bytes(bytes))
}
