use crate::dex::raydium::{clmm_info::POOL_TICK_ARRAY_BITMAP_SEED, raydium_clmm_program_id};
use solana_program::pubkey::Pubkey;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct RaydiumPool {
    pub pool: Pubkey,
    pub token_vault: Pubkey,
    pub sol_vault: Pubkey,
}

#[derive(Debug, Clone)]
pub struct RaydiumCpPool {
    pub pool: Pubkey,
    pub token_vault: Pubkey,
    pub sol_vault: Pubkey,
    pub amm_config: Pubkey,
    pub observation: Pubkey,
}

#[derive(Debug, Clone)]
pub struct PumpPool {
    pub pool: Pubkey,
    pub token_vault: Pubkey,
    pub sol_vault: Pubkey,
    pub fee_token_wallet: Pubkey,
    pub coin_creator_vault_ata: Pubkey,
    pub coin_creator_vault_authority: Pubkey,
}

#[derive(Debug, Clone)]
pub struct DlmmPool {
    pub pair: Pubkey,
    pub token_vault: Pubkey,
    pub sol_vault: Pubkey,
    pub oracle: Pubkey,
    pub bin_arrays: Vec<Pubkey>,
}

#[derive(Debug, Clone)]
pub struct WhirlpoolPool {
    pub pool: Pubkey,
    pub oracle: Pubkey,
    pub x_vault: Pubkey,
    pub y_vault: Pubkey,
    pub tick_arrays: Vec<Pubkey>,
}

#[derive(Debug, Clone)]
pub struct RaydiumClmmPool {
    pub pool: Pubkey,
    pub amm_config: Pubkey,
    pub observation_state: Pubkey,
    pub bitmap_extension: Pubkey,
    pub x_vault: Pubkey,
    pub y_vault: Pubkey,
    pub tick_arrays: Vec<Pubkey>,
}

#[derive(Debug, Clone)]
pub struct MeteoraDAmmPool {
    pub pool: Pubkey,
    pub token_x_vault: Pubkey,
    pub token_sol_vault: Pubkey,
    pub token_x_token_vault: Pubkey,
    pub token_sol_token_vault: Pubkey,
    pub token_x_lp_mint: Pubkey,
    pub token_sol_lp_mint: Pubkey,
    pub token_x_pool_lp: Pubkey,
    pub token_sol_pool_lp: Pubkey,
    pub admin_token_fee_x: Pubkey,
    pub admin_token_fee_sol: Pubkey,
}

#[derive(Debug, Clone)]
pub struct MintPoolData {
    pub mint: Pubkey,
    pub wallet_account: Pubkey,
    pub wallet_wsol_account: Pubkey,
    pub raydium_pools: Vec<RaydiumPool>,
    pub raydium_cp_pools: Vec<RaydiumCpPool>,
    pub pump_pools: Vec<PumpPool>,
    pub dlmm_pairs: Vec<DlmmPool>,
    pub whirlpool_pools: Vec<WhirlpoolPool>,
    pub raydium_clmm_pools: Vec<RaydiumClmmPool>,
    pub meteora_damm_pools: Vec<MeteoraDAmmPool>,
}

impl MintPoolData {
    pub fn new(mint: &str, wallet_account: &str) -> anyhow::Result<Self> {
        let sol_mint = spl_token::native_mint::id();
        let wallet_pk = Pubkey::from_str(wallet_account)?;
        let wallet_wsol_pk =
            spl_associated_token_account::get_associated_token_address(&wallet_pk, &sol_mint);
        Ok(Self {
            mint: Pubkey::from_str(mint)?,
            wallet_account: wallet_pk,
            wallet_wsol_account: wallet_wsol_pk,
            raydium_pools: Vec::new(),
            raydium_cp_pools: Vec::new(),
            pump_pools: Vec::new(),
            dlmm_pairs: Vec::new(),
            whirlpool_pools: Vec::new(),
            raydium_clmm_pools: Vec::new(),
            meteora_damm_pools: Vec::new(),
        })
    }

    pub fn add_raydium_pool(&mut self, pool: &Pubkey, token_vault: &Pubkey, sol_vault: &Pubkey) {
        self.raydium_pools.push(RaydiumPool {
            pool: *pool,
            token_vault: *token_vault,
            sol_vault: *sol_vault,
        });
    }

    pub fn add_raydium_cp_pool(
        &mut self,
        pool: &Pubkey,
        token_vault: &Pubkey,
        sol_vault: &Pubkey,
        amm_config: &Pubkey,
        observation: &Pubkey,
    ) {
        self.raydium_cp_pools.push(RaydiumCpPool {
            pool: *pool,
            token_vault: *token_vault,
            sol_vault: *sol_vault,
            amm_config: *amm_config,
            observation: *observation,
        });
    }

    pub fn add_pump_pool(
        &mut self,
        pool: &Pubkey,
        token_vault: &Pubkey,
        sol_vault: &Pubkey,
        fee_token_wallet: &Pubkey,
        coin_creator_vault_ata: &Pubkey,
        coin_creator_authority: &Pubkey,
    ) {
        self.pump_pools.push(PumpPool {
            pool: *pool,
            token_vault: *token_vault,
            sol_vault: *sol_vault,
            fee_token_wallet: *fee_token_wallet,
            coin_creator_vault_ata: *coin_creator_vault_ata,
            coin_creator_vault_authority: *coin_creator_authority,
        })
    }

    pub fn add_dlmm_pool(
        &mut self,
        pair: &Pubkey,
        token_vault: &Pubkey,
        sol_vault: &Pubkey,
        oracle: &Pubkey,
        bin_array_pubkeys: &[Pubkey],
    ) {
        self.dlmm_pairs.push(DlmmPool {
            pair: *pair,
            token_vault: *token_vault,
            sol_vault: *sol_vault,
            oracle: *oracle,
            bin_arrays: bin_array_pubkeys.to_vec(),
        });
    }

    pub fn add_whirlpool_pool(
        &mut self,
        pool: &Pubkey,
        oracle: &Pubkey,
        x_vault: &Pubkey,
        y_vault: &Pubkey,
        tick_arrays: &[Pubkey],
    ) {
        self.whirlpool_pools.push(WhirlpoolPool {
            pool: *pool,
            oracle: *oracle,
            x_vault: *x_vault,
            y_vault: *y_vault,
            tick_arrays: tick_arrays.to_vec(),
        });
    }

    pub fn add_raydium_clmm_pool(
        &mut self,
        pool: &Pubkey,
        amm_config: &Pubkey,
        observation_state: &Pubkey,
        x_vault: &Pubkey,
        y_vault: &Pubkey,
        tick_arrays: &[Pubkey],
    ) {
        let bitmap_extension = Pubkey::find_program_address(
            &[POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(), pool.as_ref()],
            &raydium_clmm_program_id(),
        )
        .0;

        self.raydium_clmm_pools.push(RaydiumClmmPool {
            pool: *pool,
            amm_config: *amm_config,
            observation_state: *observation_state,
            x_vault: *x_vault,
            y_vault: *y_vault,
            bitmap_extension,
            tick_arrays: tick_arrays.to_vec(),
        });
    }

    pub fn add_meteora_damm_pool(
        &mut self,
        pool: &Pubkey,
        token_x_vault: &Pubkey,
        token_sol_vault: &Pubkey,
        token_x_token_vault: &Pubkey,
        token_sol_token_vault: &Pubkey,
        token_x_lp_mint: &Pubkey,
        token_sol_lp_mint: &Pubkey,
        token_x_pool_lp: &Pubkey,
        token_sol_pool_lp: &Pubkey,
        admin_token_fee_x: &Pubkey,
        admin_token_fee_sol: &Pubkey,
    ) {
        self.meteora_damm_pools.push(MeteoraDAmmPool {
            pool: *pool,
            token_x_vault: *token_x_vault,
            token_sol_vault: *token_sol_vault,
            token_x_token_vault: *token_x_token_vault,
            token_sol_token_vault: *token_sol_token_vault,
            token_x_lp_mint: *token_x_lp_mint,
            token_sol_lp_mint: *token_sol_lp_mint,
            token_x_pool_lp: *token_x_pool_lp,
            token_sol_pool_lp: *token_sol_pool_lp,
            admin_token_fee_x: *admin_token_fee_x,
            admin_token_fee_sol: *admin_token_fee_sol,
        })
    }
}
