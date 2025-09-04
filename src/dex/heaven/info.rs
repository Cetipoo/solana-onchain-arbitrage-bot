use solana_program::pubkey::Pubkey;

#[derive(Debug, Clone)]
pub struct HeavenPoolState {
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    pub vault_a: Pubkey,
    pub vault_b: Pubkey,
    pub protocol_config: Pubkey,
    pub reserve_a: u64,
    pub reserve_b: u64,
}

impl HeavenPoolState {
    pub const DISCRIMINATOR: [u8; 8] = [190, 158, 220, 130, 15, 162, 132, 252];
    pub const SIZE: usize = 2304;

    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != Self::SIZE {
            return None;
        }

        // Check discriminator
        if &data[0..8] != &Self::DISCRIMINATOR {
            return None;
        }

        // Offsets based on Heaven pool structure (copied from reference)
        const OFFSET_RESERVE: usize = 8 + 88 + 360;
        const OFFSET_VAULTS: usize = 8 + 88 + 360 + 72 + 48 + 64 + 24;
        const OFFSET_PROTOCOL_CONFIG: usize = OFFSET_VAULTS + 64;
        const OFFSET_TOKEN_A: usize = OFFSET_VAULTS + 128;
        const OFFSET_TOKEN_B: usize = OFFSET_TOKEN_A + 65;

        let reserve_a = u64::from_le_bytes(data[OFFSET_RESERVE..OFFSET_RESERVE + 8].try_into().ok()?);
        let reserve_b = u64::from_le_bytes(
            data[OFFSET_RESERVE + 8..OFFSET_RESERVE + 16]
                .try_into()
                .ok()?,
        );

        let vault_a = Pubkey::try_from(&data[OFFSET_VAULTS..OFFSET_VAULTS + 32]).ok()?;
        let vault_b = Pubkey::try_from(&data[OFFSET_VAULTS + 32..OFFSET_VAULTS + 64]).ok()?;
        let protocol_config =
            Pubkey::try_from(&data[OFFSET_PROTOCOL_CONFIG..OFFSET_PROTOCOL_CONFIG + 32]).ok()?;

        let mint_a = Pubkey::try_from(&data[OFFSET_TOKEN_A..OFFSET_TOKEN_A + 32]).ok()?;
        let mint_b = Pubkey::try_from(&data[OFFSET_TOKEN_B..OFFSET_TOKEN_B + 32]).ok()?;

        Some(Self {
            mint_a,
            mint_b,
            vault_a,
            vault_b,
            protocol_config,
            reserve_a,
            reserve_b,
        })
    }

    pub fn get_token_vault(&self, token_mint: &Pubkey) -> Option<Pubkey> {
        if &self.mint_a == token_mint {
            Some(self.vault_a)
        } else if &self.mint_b == token_mint {
            Some(self.vault_b)
        } else {
            None
        }
    }

    pub fn get_base_vault(&self, token_mint: &Pubkey) -> Option<Pubkey> {
        if &self.mint_a == token_mint {
            Some(self.vault_b)
        } else if &self.mint_b == token_mint {
            Some(self.vault_a)
        } else {
            None
        }
    }

    pub fn get_base_mint(&self, token_mint: &Pubkey) -> Option<Pubkey> {
        if &self.mint_a == token_mint {
            Some(self.mint_b)
        } else if &self.mint_b == token_mint {
            Some(self.mint_a)
        } else {
            None
        }
    }
}