//! V10 instruction builder for the on-chain arbitrage program.
use anyhow::{bail, ensure, Result};
use executor_v10_abi::{Group, InstructionData, Venue, MAX_GROUPS, MAX_PAYLOAD_LEN, MAX_POOLS};
pub use executor_v10_abi::{Header, OPCODE};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey,
    pubkey::Pubkey,
};

pub const CPMM: Pubkey = pubkey!("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C");
pub const CLMM: Pubkey = pubkey!("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");
pub const PANCAKESWAP: Pubkey = pubkey!("HpNfyc2Saw7RKkQd8nEL4khUcuPhQ7WwY1B2qjx8jxFq");
pub const BYREAL: Pubkey = pubkey!("REALQqNEomY6cQGZJUGwywTBD2UmDT32rZcNnfxQ5N2");
pub const WHIRLPOOL: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
pub const DLMM: Pubkey = pubkey!("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");
pub const PUMP: Pubkey = pubkey!("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
pub const PUMP_FEE: Pubkey = pubkey!("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");
pub const CLMM_CONVERSION: Pubkey = pubkey!("3ucNos4NbumPLZNWztqGHNFFgkHeRMBQAVemeeomsUxv");
pub const RAYDIUM: Pubkey = pubkey!("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");
pub const RAYDIUM_CONVERSION: Pubkey = pubkey!("58oQChx4yWmvKdwLLZzBi4ChoCc2fqCUWBkwMihLYQo2");
pub const ORCA_CONVERSION: Pubkey = pubkey!("Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE");

pub const MANIFEST: Pubkey = pubkey!("MNFSTqtC93rEfYHB6hF82sKdZpUDFWkViLByLd1k1Ms");
pub const DAMMV2: Pubkey = pubkey!("cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG");
/// Meteora DAMM v1 (dynamic pools) and the vault program its swaps deposit into.
pub const METEORA: Pubkey = pubkey!("Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB");
pub const METEORA_VAULT: Pubkey = pubkey!("24Uqj9JCLxUeoC3hGfh5W3s9FM9uCHDS2SG3LYwBpyTi");

pub const SOL: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
pub const USDC: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
pub const TOKEN: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const TOKEN_2022: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
pub const MEMO: Pubkey = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
pub const ASSOCIATED: Pubkey = pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
pub const SYSTEM: Pubkey = pubkey!("11111111111111111111111111111111");

#[cfg(test)]
#[path = "v10_tests.rs"]
mod tests;

fn read<const N: usize>(d: &[u8], offset: usize) -> Result<[u8; N]> {
    d.get(offset..offset + N)
        .ok_or_else(|| anyhow::anyhow!("truncated pool account"))?
        .try_into()
        .map_err(Into::into)
}

pub fn array_candidates(
    pool: Pubkey,
    owner: Pubkey,
    data: &[u8],
    radius: u8,
) -> Result<(Vec<Pubkey>, Option<Pubkey>)> {
    ensure!(radius <= 2, "array radius must be 0, 1 or 2");
    let (current, width, seed, bitmap) = match owner {
        RAYDIUM | CPMM | DAMMV2 | PUMP | METEORA | MANIFEST => return Ok((vec![], None)),
        CLMM | PANCAKESWAP | BYREAL => {
            let spacing = u16::from_le_bytes(read(data, 235)?) as i32;
            ensure!(spacing > 0, "invalid CLMM spacing");
            (
                i32::from_le_bytes(read(data, 269)?),
                spacing * 60,
                b"tick_array".as_slice(),
                Some(
                    Pubkey::find_program_address(
                        &[b"pool_tick_array_bitmap_extension", pool.as_ref()],
                        &owner,
                    )
                    .0,
                ),
            )
        }
        WHIRLPOOL => {
            let spacing = u16::from_le_bytes(read(data, 41)?) as i32;
            ensure!(spacing > 0, "invalid Whirlpool spacing");
            (
                i32::from_le_bytes(read(data, 81)?),
                spacing * 88,
                b"tick_array".as_slice(),
                None,
            )
        }
        DLMM => (
            i32::from_le_bytes(read(data, 76)?),
            70,
            b"bin_array".as_slice(),
            Some(Pubkey::find_program_address(&[b"bitmap", pool.as_ref()], &owner).0),
        ),
        _ => bail!("unsupported V10 DEX {}", owner),
    };
    let center = current.div_euclid(width);
    let arrays = (-(radius as i32)..=radius as i32)
        .map(|offset| {
            let index = center + offset;
            if owner == DLMM {
                Pubkey::find_program_address(
                    &[seed, pool.as_ref(), &(index as i64).to_le_bytes()],
                    &owner,
                )
                .0
            } else if [CLMM, PANCAKESWAP, BYREAL].contains(&owner) {
                Pubkey::find_program_address(
                    &[seed, pool.as_ref(), &(index * width).to_be_bytes()],
                    &owner,
                )
                .0
            } else {
                Pubkey::find_program_address(
                    &[seed, pool.as_ref(), (index * width).to_string().as_bytes()],
                    &owner,
                )
                .0
            }
        })
        .collect();
    Ok((arrays, bitmap))
}

#[derive(Clone, Copy, Debug)]
pub struct MintAccounts {
    pub mint: Pubkey,
    pub token_program: Pubkey,
    pub wallet: Pubkey,
}
impl MintAccounts {
    pub fn ata(wallet: &Pubkey, mint: Pubkey, token_program: Pubkey) -> Result<Self> {
        ensure!(
            [TOKEN, TOKEN_2022].contains(&token_program),
            "unsupported token program"
        );
        Ok(Self {
            mint,
            token_program,
            wallet: Pubkey::find_program_address(
                &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
                &ASSOCIATED,
            )
            .0,
        })
    }
    fn append(self, to: &mut Vec<AccountMeta>) {
        to.extend([
            AccountMeta::new_readonly(self.mint, false),
            AccountMeta::new_readonly(self.token_program, false),
            AccountMeta::new(self.wallet, false),
        ]);
    }
}

/// The executor rejects a Manifest market unless its free list holds a node
/// past the one a resting order would take, so a swap never grows the market.
fn manifest_has_spare_node(data: &[u8]) -> bool {
    let node = |index: u32| {
        let start = 256 + index as usize;
        (index != u32::MAX && index.is_multiple_of(80))
            .then(|| data.get(start..start + 80))
            .flatten()
    };
    let link = |index: u32| Some(u32::from_le_bytes(read(node(index)?, 0).ok()?));
    let Ok(free) = read(data, 176).map(u32::from_le_bytes) else {
        return false;
    };
    link(free).is_some_and(|next| next != free && node(next).is_some())
}

/// Canonical mint/vault layout shared by pool discovery and instruction construction.
/// For Meteora DAMM v1 the vaults are the vault-program accounts, not token accounts.
pub struct PoolPair {
    pub mints: [Pubkey; 2],
    vaults: [Pubkey; 2],
}
impl PoolPair {
    pub fn read(program: Pubkey, data: &[u8]) -> Result<Self> {
        let (m0, m1, v0, v1) = match program {
            RAYDIUM => (400, 432, 336, 368),
            METEORA => (40, 72, 104, 136),
            CPMM => (168, 200, 72, 104),
            CLMM | PANCAKESWAP | BYREAL => (73, 105, 137, 169),
            WHIRLPOOL => (101, 181, 133, 213),
            DLMM => (88, 120, 152, 184),
            MANIFEST => {
                ensure!(
                    data.len() >= 256
                        && data[8] == 0
                        && u64::from_le_bytes(read(data, 0)?) == 4859840929024028656,
                    "invalid Manifest market"
                );
                (16, 48, 80, 112)
            }
            DAMMV2 => (168, 200, 232, 264),
            PUMP => (43, 75, 139, 171),
            _ => bail!("unsupported V10 DEX {program}"),
        };
        let key = |offset| -> Result<Pubkey> { Ok(Pubkey::new_from_array(read(data, offset)?)) };
        Ok(Self {
            mints: [key(m0)?, key(m1)?],
            vaults: [key(v0)?, key(v1)?],
        })
    }

    pub fn vaults(&self) -> [Pubkey; 2] {
        self.vaults
    }

    fn vaults_for(&self, x: Pubkey, base: Pubkey) -> Result<[Pubkey; 2]> {
        ensure!(x != base, "pool mints must differ");
        if self.mints == [x, base] {
            Ok(self.vaults)
        } else if self.mints == [base, x] {
            Ok([self.vaults[1], self.vaults[0]])
        } else {
            bail!("pool does not match the declared mint pair")
        }
    }
}

/// DEX-specific accounts for a decoded pool. Vaults are supplied in X/base order.
#[derive(Clone, Copy, Debug)]
pub enum PoolKeys<'a> {
    Raydium,
    Cpmm {
        config: Pubkey,
        observation: Pubkey,
    },
    DammV2,
    Manifest,
    Clmm {
        program: Pubkey,
        config: Pubkey,
        observation: Pubkey,
        bitmap: Pubkey,
        arrays: &'a [Pubkey],
    },
    Whirlpool {
        arrays: &'a [Pubkey],
    },
    Dlmm {
        oracle: Pubkey,
        bitmap: Option<Pubkey>,
        arrays: &'a [Pubkey],
    },
}
impl PoolKeys<'_> {
    fn program(self) -> Pubkey {
        match self {
            Self::Raydium => RAYDIUM,
            Self::Cpmm { .. } => CPMM,
            Self::DammV2 => DAMMV2,
            Self::Manifest => MANIFEST,
            Self::Clmm { program, .. } => program,
            Self::Whirlpool { .. } => WHIRLPOOL,
            Self::Dlmm { .. } => DLMM,
        }
    }

    /// The venue the executor quotes the pool as.
    fn venue(self) -> Venue {
        match self {
            Self::Raydium => Venue::RaydiumAmm,
            Self::Cpmm { .. } => Venue::RaydiumCpmm,
            Self::DammV2 => Venue::Dammv2,
            Self::Manifest => Venue::Manifest,
            Self::Clmm { program, .. } if program == CLMM => Venue::Clmm,
            Self::Clmm { .. } => Venue::ClmmFork,
            Self::Whirlpool { .. } => Venue::Whirlpool,
            Self::Dlmm { .. } => Venue::Dlmm,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PumpKeys {
    pub vaults: [Pubkey; 2],
    pub mint0: Pubkey,
    pub quote: MintAccounts,
    pub recipient: Pubkey,
    pub creator: Pubkey,
    pub cashback: bool,
    pub buyback: Pubkey,
}

impl PumpKeys {
    pub fn from_state(
        data: &[u8],
        global: &[u8],
        x: MintAccounts,
        base: MintAccounts,
    ) -> Result<Self> {
        let key = |offset| -> Result<Pubkey> { Ok(Pubkey::new_from_array(read(data, offset)?)) };
        let pair = PoolPair::read(PUMP, data)?;
        let [mint0, mint1] = pair.mints;
        let [x_vault, base_vault] = pair.vaults_for(x.mint, base.mint)?;
        let quote = if mint1 == base.mint { base } else { x };
        let recipient_offset = if *data
            .get(243)
            .ok_or_else(|| anyhow::anyhow!("truncated Pump pool"))?
            == 0
        {
            57
        } else {
            385
        };
        let recipient = Pubkey::new_from_array(read(global, recipient_offset)?);
        Ok(Self {
            vaults: [x_vault, base_vault],
            mint0,
            quote,
            recipient,
            creator: key(211)?,
            cashback: data.get(244) == Some(&1),
            buyback: Pubkey::new_from_array(read(global, 643)?),
        })
    }
}

#[derive(Clone, Debug)]
pub struct PoolAccounts {
    venue: Venue,
    pool: Pubkey,
    x_mint: Pubkey,
    base_mint: Pubkey,
    accounts: Vec<AccountMeta>,
    /// Pump's orientation; `None` for every other venue.
    pump_quote_is_base: Option<bool>,
}
impl PoolAccounts {
    pub fn from_pump_state(
        pool: Pubkey,
        data: &[u8],
        global: &[u8],
        wallet: Pubkey,
        x: MintAccounts,
        base: MintAccounts,
    ) -> Result<Self> {
        Self::from_pump_keys(
            pool,
            wallet,
            x,
            base,
            PumpKeys::from_state(data, global, x, base)?,
        )
    }

    pub fn from_pump_keys(
        pool: Pubkey,
        wallet: Pubkey,
        x: MintAccounts,
        base: MintAccounts,
        keys: PumpKeys,
    ) -> Result<Self> {
        ensure!(
            [x.mint, base.mint].contains(&keys.quote.mint)
                && [x.mint, base.mint].contains(&keys.mint0)
                && keys.quote.mint != keys.mint0,
            "invalid Pump orientation"
        );
        let [x_vault, base_vault] = keys.vaults;
        let PumpKeys {
            mint0,
            quote,
            recipient,
            ..
        } = keys;
        let pda = |seeds: &[&[u8]]| Pubkey::find_program_address(seeds, &PUMP).0;
        let ata = |owner: Pubkey| {
            Pubkey::find_program_address(
                &[
                    owner.as_ref(),
                    quote.token_program.as_ref(),
                    quote.mint.as_ref(),
                ],
                &ASSOCIATED,
            )
            .0
        };
        let creator = pda(&[b"creator_vault", keys.creator.as_ref()]);
        let mut accounts = vec![
            AccountMeta::new_readonly(PUMP, false),
            AccountMeta::new_readonly(base.mint, false),
            AccountMeta::new_readonly(pda(&[b"global_config"]), false),
            AccountMeta::new_readonly(pda(&[b"__event_authority"]), false),
            AccountMeta::new_readonly(recipient, false),
            AccountMeta::new(pool, false),
            AccountMeta::new(x_vault, false),
            AccountMeta::new(base_vault, false),
            AccountMeta::new(ata(recipient), false),
            AccountMeta::new(ata(creator), false),
            AccountMeta::new_readonly(creator, false),
            AccountMeta::new_readonly(pda(&[b"global_volume_accumulator"]), false),
            AccountMeta::new(pda(&[b"user_volume_accumulator", wallet.as_ref()]), false),
            AccountMeta::new_readonly(
                Pubkey::find_program_address(&[b"fee_config", PUMP.as_ref()], &PUMP_FEE).0,
                false,
            ),
            AccountMeta::new_readonly(PUMP_FEE, false),
        ];
        if keys.cashback {
            let volume = pda(&[b"user_volume_accumulator", wallet.as_ref()]);
            accounts.extend([
                AccountMeta::new(ata(volume), false),
                AccountMeta::new(volume, false),
            ]);
        }
        if keys.creator != Pubkey::default() {
            accounts.push(AccountMeta::new_readonly(
                pda(&[b"pool-v2", mint0.as_ref()]),
                false,
            ));
        }
        let buyback = keys.buyback;
        accounts.extend([
            AccountMeta::new_readonly(buyback, false),
            AccountMeta::new(ata(buyback), false),
        ]);
        Ok(Self {
            venue: Venue::Pump,
            pool,
            x_mint: x.mint,
            base_mint: base.mint,
            accounts,
            pump_quote_is_base: Some(quote.mint == base.mint),
        })
    }

    pub fn supports_meteora_state(data: &[u8]) -> bool {
        data.get(233) == Some(&1) && data.get(874) == Some(&0)
    }

    /// Meteora DAMM v1 reads each vault's token account and LP mint from the
    /// vault-program state, so both vault accounts accompany the pool state.
    pub fn from_meteora_state(
        pool: Pubkey,
        data: &[u8],
        a_vault: &[u8],
        b_vault: &[u8],
        x: MintAccounts,
        base: MintAccounts,
    ) -> Result<Self> {
        ensure!(
            x.token_program == TOKEN && base.token_program == TOKEN,
            "V10 Meteora DAMM v1 requires legacy token mints"
        );
        ensure!(
            Self::supports_meteora_state(data),
            "V10 Meteora DAMM v1 requires an enabled constant-product pool"
        );
        let pair = PoolPair::read(METEORA, data)?;
        let base_is_a = if pair.mints == [base.mint, x.mint] {
            true
        } else if pair.mints == [x.mint, base.mint] {
            false
        } else {
            bail!("pool does not match the declared mint pair");
        };
        let key =
            |d: &[u8], offset| -> Result<Pubkey> { Ok(Pubkey::new_from_array(read(d, offset)?)) };
        // Per token: vault, token vault, LP mint, pool LP account, admin fee account.
        let side = |state: &[u8], index: usize, pool_lp, fee| -> Result<[Pubkey; 5]> {
            ensure!(
                key(state, 83)? == pair.mints[index],
                "Meteora vault mint mismatch"
            );
            Ok([
                pair.vaults[index],
                key(state, 19)?,
                key(state, 115)?,
                key(data, pool_lp)?,
                key(data, fee)?,
            ])
        };
        let a = side(a_vault, 0, 168, 234)?;
        let b = side(b_vault, 1, 200, 266)?;
        let (x_side, base_side) = if base_is_a { (b, a) } else { (a, b) };
        Self::from_meteora_keys(pool, x, base, x_side, base_side)
    }

    /// Each side is vault, token vault, LP mint, pool LP account, admin fee account.
    pub fn from_meteora_keys(
        pool: Pubkey,
        x: MintAccounts,
        base: MintAccounts,
        x_side: [Pubkey; 5],
        base_side: [Pubkey; 5],
    ) -> Result<Self> {
        ensure!(
            x.token_program == TOKEN && base.token_program == TOKEN,
            "V10 Meteora DAMM v1 requires legacy token mints"
        );
        let mut accounts = vec![
            AccountMeta::new_readonly(METEORA, false),
            AccountMeta::new_readonly(base.mint, false),
            AccountMeta::new_readonly(METEORA_VAULT, false),
            AccountMeta::new(pool, false),
        ];
        accounts.extend(
            x_side
                .into_iter()
                .zip(base_side)
                .flat_map(|(x, base)| [AccountMeta::new(x, false), AccountMeta::new(base, false)]),
        );
        Ok(Self {
            venue: Venue::MeteoraDamm,
            pool,
            x_mint: x.mint,
            base_mint: base.mint,
            accounts,
            pump_quote_is_base: None,
        })
    }

    pub fn venue(&self) -> Venue {
        self.venue
    }
    pub fn pump_quote_is_base(&self) -> Option<bool> {
        self.pump_quote_is_base
    }
    pub fn pool(&self) -> Pubkey {
        self.pool
    }
    pub fn accounts(&self) -> &[AccountMeta] {
        &self.accounts
    }
    pub fn x_mint(&self) -> Pubkey {
        self.x_mint
    }
    pub fn base_mint(&self) -> Pubkey {
        self.base_mint
    }

    /// The state and arrays must come from the same account snapshot. Pool mints determine orientation.
    pub fn from_state(
        pool: Pubkey,
        owner: Pubkey,
        data: &[u8],
        x: MintAccounts,
        base: MintAccounts,
        arrays: &[Pubkey],
        bitmap: Option<Pubkey>,
    ) -> Result<Self> {
        let key = |offset: usize| -> Result<Pubkey> {
            Ok(Pubkey::new_from_array(
                data.get(offset..offset + 32)
                    .ok_or_else(|| anyhow::anyhow!("truncated pool {pool}"))?
                    .try_into()?,
            ))
        };
        ensure!(
            owner != PUMP,
            "Pump requires from_pump_state with its global config"
        );
        ensure!(
            owner != METEORA,
            "Meteora DAMM v1 requires from_meteora_state with its vault states"
        );
        let pair = PoolPair::read(owner, data)?;
        let [x_vault, base_vault] = pair.vaults_for(x.mint, base.mint)?;
        let keys = match owner {
            RAYDIUM | CPMM | DAMMV2 | MANIFEST => {
                ensure!(
                    arrays.is_empty() && bitmap.is_none(),
                    "pool does not use arrays"
                );
                match owner {
                    MANIFEST => {
                        ensure!(
                            manifest_has_spare_node(data),
                            "Manifest market {pool} has no spare order node"
                        );
                        PoolKeys::Manifest
                    }
                    RAYDIUM => PoolKeys::Raydium,
                    CPMM => PoolKeys::Cpmm {
                        config: key(8)?,
                        observation: key(296)?,
                    },
                    _ => PoolKeys::DammV2,
                }
            }
            CLMM | PANCAKESWAP | BYREAL => PoolKeys::Clmm {
                program: owner,
                config: key(9)?,
                observation: key(201)?,
                bitmap: bitmap.ok_or_else(|| anyhow::anyhow!("CLMM bitmap address required"))?,
                arrays,
            },
            WHIRLPOOL => {
                ensure!(bitmap.is_none(), "Whirlpool has no bitmap extension");
                PoolKeys::Whirlpool { arrays }
            }
            DLMM => PoolKeys::Dlmm {
                oracle: key(552)?,
                bitmap,
                arrays,
            },
            _ => bail!("unsupported V10 pool program"),
        };
        Self::from_keys(pool, x, base, [x_vault, base_vault], keys)
    }

    /// Encode decoded pool metadata using the same layout as `from_state`.
    pub fn from_keys(
        pool: Pubkey,
        x: MintAccounts,
        base: MintAccounts,
        vaults: [Pubkey; 2],
        keys: PoolKeys<'_>,
    ) -> Result<Self> {
        ensure!(x.mint != base.mint, "pool mints must differ");
        let owner = keys.program();
        let [x_vault, base_vault] = vaults;
        let mut accounts = vec![
            AccountMeta::new_readonly(owner, false),
            AccountMeta::new_readonly(base.mint, false),
        ];
        let v2 = x.token_program == TOKEN_2022 || base.token_program == TOKEN_2022;
        match keys {
            PoolKeys::Manifest => {
                accounts.extend([
                    AccountMeta::new(pool, false),
                    AccountMeta::new(x_vault, false),
                    AccountMeta::new(base_vault, false),
                ]);
            }
            PoolKeys::Raydium => {
                ensure!(!v2, "V10 Raydium requires legacy token mints and no arrays");
                accounts.extend([
                    AccountMeta::new_readonly(
                        Pubkey::find_program_address(&[b"amm authority"], &owner).0,
                        false,
                    ),
                    AccountMeta::new(pool, false),
                    AccountMeta::new(x_vault, false),
                    AccountMeta::new(base_vault, false),
                ]);
            }
            PoolKeys::DammV2 => {
                accounts.extend([
                    AccountMeta::new_readonly(
                        Pubkey::find_program_address(&[b"__event_authority"], &owner).0,
                        false,
                    ),
                    AccountMeta::new_readonly(
                        Pubkey::find_program_address(&[b"pool_authority"], &owner).0,
                        false,
                    ),
                    AccountMeta::new(pool, false),
                    AccountMeta::new(x_vault, false),
                    AccountMeta::new(base_vault, false),
                ]);
            }
            PoolKeys::Cpmm {
                config,
                observation,
            } => {
                accounts.extend([
                    AccountMeta::new_readonly(
                        Pubkey::find_program_address(&[b"vault_and_lp_mint_auth_seed"], &CPMM).0,
                        false,
                    ),
                    AccountMeta::new(pool, false),
                    AccountMeta::new_readonly(config, false),
                    AccountMeta::new(x_vault, false),
                    AccountMeta::new(base_vault, false),
                    AccountMeta::new(observation, false),
                ]);
            }
            PoolKeys::Clmm { arrays, .. }
            | PoolKeys::Whirlpool { arrays }
            | PoolKeys::Dlmm { arrays, .. } => {
                ensure!(
                    !arrays.is_empty() && arrays.len() <= 6,
                    "V10 requires 1..=6 arrays per concentrated pool"
                );
                if owner == DLMM {
                    accounts.push(AccountMeta::new_readonly(
                        Pubkey::find_program_address(&[b"__event_authority"], &DLMM).0,
                        false,
                    ));
                }
                // Whirlpool V2 supports both token programs and writable oracle accounts.
                if v2 || owner == WHIRLPOOL {
                    accounts.push(AccountMeta::new_readonly(MEMO, false));
                }
                accounts.push(AccountMeta::new(pool, false));
                match keys {
                    PoolKeys::Clmm {
                        program,
                        config,
                        observation,
                        bitmap,
                        ..
                    } => {
                        ensure!(
                            [CLMM, PANCAKESWAP, BYREAL].contains(&program),
                            "unsupported CLMM program"
                        );
                        accounts.extend([
                            AccountMeta::new_readonly(config, false),
                            AccountMeta::new(observation, false),
                            AccountMeta::new(bitmap, false),
                            AccountMeta::new(x_vault, false),
                            AccountMeta::new(base_vault, false),
                        ]);
                    }
                    PoolKeys::Whirlpool { .. } => {
                        accounts.extend([
                            AccountMeta::new(
                                Pubkey::find_program_address(
                                    &[b"oracle", pool.as_ref()],
                                    &WHIRLPOOL,
                                )
                                .0,
                                false,
                            ),
                            AccountMeta::new(x_vault, false),
                            AccountMeta::new(base_vault, false),
                        ]);
                    }
                    PoolKeys::Dlmm { oracle, bitmap, .. } => {
                        accounts.extend([
                            AccountMeta::new(x_vault, false),
                            AccountMeta::new(base_vault, false),
                            AccountMeta::new(oracle, false),
                        ]);
                        if let Some(bitmap) = bitmap {
                            accounts.push(AccountMeta::new(bitmap, false));
                        }
                    }
                    _ => unreachable!(),
                }
                accounts.extend(arrays.iter().map(|&a| AccountMeta::new(a, false)));
            }
        }
        Ok(Self {
            venue: keys.venue(),
            pool,
            x_mint: x.mint,
            base_mint: base.mint,
            accounts,
            pump_quote_is_base: None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct MarketGroup {
    pub target: MintAccounts,
    pub base: MintAccounts,
    pub intermediate: PoolAccounts,
    pub bridges: Vec<PoolAccounts>,
    pub direct: Vec<PoolAccounts>,
}

#[derive(Clone, Debug)]
pub struct DirectGroup {
    pub target: MintAccounts,
    pub pools: Vec<PoolAccounts>,
}

#[derive(Clone, Debug)]
pub struct ConversionAccounts {
    pub quote: MintAccounts,
    pub pool: PoolAccounts,
}

#[derive(Clone, Debug)]
pub struct V10Instruction {
    pub program_id: Pubkey,
    pub wallet: Pubkey,
    pub settlement: MintAccounts,
    pub conversion: Option<ConversionAccounts>,
    pub fee_collector: Pubkey,
    pub additional_fee_collector: Option<Pubkey>,
    pub flashloan: Option<(Pubkey, Pubkey)>,
    pub header: Header,
    pub groups: Vec<MarketGroup>,
}
impl V10Instruction {
    pub fn build(&self) -> Result<Instruction> {
        self.build_groups(None)
    }

    /// Direct groups only; the request's triangle groups must be empty.
    pub fn build_direct(&self, groups: &[DirectGroup]) -> Result<Instruction> {
        ensure!(
            self.groups.is_empty(),
            "direct and triangle groups cannot be combined"
        );
        self.build_groups(Some(groups))
    }

    fn build_groups(&self, direct: Option<&[DirectGroup]>) -> Result<Instruction> {
        ensure!(
            [SOL, USDC].contains(&self.settlement.mint),
            "settlement must be WSOL or USDC"
        );
        ensure!(
            self.settlement.token_program == TOKEN,
            "invalid settlement token program"
        );
        ensure!(
            self.header.use_flashloan == self.flashloan.is_some(),
            "flashloan header/accounts mismatch"
        );
        ensure!(
            (self.header.additional_fee_bp > 0) == self.additional_fee_collector.is_some(),
            "additional fee header/accounts mismatch"
        );
        let group_count = direct.map_or(self.groups.len(), <[DirectGroup]>::len);
        ensure!(
            group_count > 0 && group_count <= MAX_GROUPS,
            "invalid V10 group count"
        );
        let mut data = InstructionData {
            header: self.header,
            group_count: group_count as u8,
            ..InstructionData::default()
        };
        let mut accounts = vec![
            AccountMeta::new(self.wallet, true),
            AccountMeta::new_readonly(self.settlement.mint, false),
            AccountMeta::new(self.fee_collector, false),
        ];
        if let Some(fee) = self.additional_fee_collector {
            accounts.push(AccountMeta::new(fee, false));
        }
        accounts.extend([
            AccountMeta::new(self.settlement.wallet, false),
            AccountMeta::new_readonly(TOKEN, false),
            AccountMeta::new_readonly(SYSTEM, false),
            AccountMeta::new_readonly(ASSOCIATED, false),
        ]);
        if let Some((authority, vault)) = self.flashloan {
            accounts.extend([
                AccountMeta::new_readonly(authority, false),
                AccountMeta::new(vault, false),
            ]);
        }
        let quote = if let Some(c) = &self.conversion {
            ensure!(
                [SOL, USDC].contains(&c.quote.mint)
                    && c.quote.mint != self.settlement.mint
                    && c.quote.token_program == TOKEN,
                "conversion must connect WSOL and USDC"
            );
            ensure!(
                c.pool.x_mint == c.quote.mint && c.pool.base_mint == self.settlement.mint,
                "invalid settlement conversion pool"
            );
            data.conversion_account_count = c.pool.accounts.len().try_into()?;
            c.quote.append(&mut accounts);
            accounts.extend_from_slice(&c.pool.accounts);
            c.quote
        } else {
            self.settlement
        };
        let mut total = usize::from(self.conversion.is_some());
        for (i, group) in direct.unwrap_or_default().iter().enumerate() {
            ensure!(
                group.target.mint != self.settlement.mint && group.target.mint != quote.mint,
                "direct target must differ from settlement and conversion quote"
            );
            ensure!(
                group.pools.len() >= 2,
                "direct groups require at least two pools"
            );
            ensure!(
                group.pools.iter().any(|p| p.base_mint == self.settlement.mint),
                "direct groups currently require a settlement-quoted pool; repeated conversion is not implemented"
            );
            total += group.pools.len();
            ensure!(total <= MAX_POOLS, "too many V10 pools");
            group.target.append(&mut accounts);
            let desc = &mut data.groups[i];
            desc.direct_count = group.pools.len() as u8;
            for (j, pool) in group.pools.iter().enumerate() {
                ensure!(
                    pool.x_mint == group.target.mint
                        && [self.settlement.mint, quote.mint].contains(&pool.base_mint),
                    "incorrect direct pool mint pair"
                );
                desc.pool_account_counts[j] = pool.accounts.len().try_into()?;
                accounts.extend_from_slice(&pool.accounts);
            }
        }
        for (i, g) in self.groups.iter().enumerate() {
            ensure!(
                g.target.mint != g.base.mint
                    && ![g.target.mint, g.base.mint].contains(&self.settlement.mint)
                    && ![g.target.mint, g.base.mint].contains(&quote.mint),
                "triangle mints must differ"
            );
            ensure!(
                !g.bridges.is_empty() && !g.direct.is_empty(),
                "bridges and direct candidates required"
            );
            let count = 1 + g.bridges.len() + g.direct.len();
            total += count;
            ensure!(
                total <= MAX_POOLS,
                "V10 permits at most {MAX_POOLS} pool declarations"
            );
            let mut desc = Group {
                bridge_count: g.bridges.len() as u8,
                direct_count: g.direct.len() as u8,
                ..Group::default()
            };
            g.target.append(&mut accounts);
            g.base.append(&mut accounts);
            for (j, p) in core::iter::once(&g.intermediate)
                .chain(g.bridges.iter())
                .chain(g.direct.iter())
                .enumerate()
            {
                let (x, b) = if j == 0 {
                    (g.target.mint, g.base.mint)
                } else if j <= g.bridges.len() {
                    (g.base.mint, quote.mint)
                } else {
                    (g.target.mint, self.settlement.mint)
                };
                ensure!(
                    p.x_mint == x && p.base_mint == b,
                    "pool {} has incorrect V10 role",
                    p.pool
                );
                desc.pool_account_counts[j] = p.accounts.len().try_into()?;
                accounts.extend_from_slice(&p.accounts);
            }
            data.groups[i] = desc;
        }
        let mut buffer = [0; MAX_PAYLOAD_LEN];
        let payload = data
            .encode(&mut buffer)
            .map_err(|_| anyhow::anyhow!("invalid V10 payload bounds"))?;
        let mut bytes = vec![OPCODE];
        bytes.extend_from_slice(payload);
        ensure!(
            accounts.len() == data.account_count(),
            "V10 account count mismatch"
        );
        Ok(Instruction {
            program_id: self.program_id,
            accounts,
            data: bytes,
        })
    }
}
