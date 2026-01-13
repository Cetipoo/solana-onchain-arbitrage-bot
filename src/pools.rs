use crate::{
    constants::sol_mint,
    dex::raydium::{clmm_info::POOL_TICK_ARRAY_BITMAP_SEED, raydium_clmm_program_id},
};
use solana_program::instruction::AccountMeta;
use solana_program::pubkey::Pubkey;

#[derive(Debug, Clone)]
pub struct RaydiumPool {
    pub pool: Pubkey,
    pub token_vault: Pubkey,
    pub sol_vault: Pubkey,
    pub token_mint: Pubkey,
    pub base_mint: Pubkey,
}

#[derive(Debug, Clone)]
pub struct RaydiumCpPool {
    pub pool: Pubkey,
    pub token_vault: Pubkey,
    pub sol_vault: Pubkey,
    pub amm_config: Pubkey,
    pub observation: Pubkey,
    pub token_mint: Pubkey,
    pub base_mint: Pubkey,
}

#[derive(Debug, Clone)]
pub struct PumpPool {
    pub pool: Pubkey,
    pub token_vault: Pubkey,
    pub sol_vault: Pubkey,
    pub fee_wallet: Pubkey,
    pub fee_token_wallet: Pubkey,
    pub coin_creator_vault_ata: Pubkey,
    pub coin_creator_vault_authority: Pubkey,
    pub token_mint: Pubkey,
    pub base_mint: Pubkey,
    pub is_mayhem_mode: bool,
}

#[derive(Debug, Clone)]
pub struct DlmmPool {
    pub pair: Pubkey,
    pub token_vault: Pubkey,
    pub sol_vault: Pubkey,
    pub oracle: Pubkey,
    pub bin_arrays: Vec<Pubkey>,
    pub memo_program: Option<Pubkey>, // For Token 2022 support
    pub token_mint: Pubkey,
    pub base_mint: Pubkey,
}

#[derive(Debug, Clone)]
pub struct WhirlpoolPool {
    pub pool: Pubkey,
    pub oracle: Pubkey,
    pub x_vault: Pubkey,
    pub y_vault: Pubkey,
    pub tick_arrays: Vec<Pubkey>,
    pub memo_program: Option<Pubkey>, // For Token 2022 support
    pub token_mint: Pubkey,
    pub base_mint: Pubkey,
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
    pub memo_program: Option<Pubkey>, // For Token 2022 support
    pub token_mint: Pubkey,
    pub base_mint: Pubkey,
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
    pub token_mint: Pubkey,
    pub base_mint: Pubkey,
}

#[derive(Debug, Clone)]
pub struct MeteoraDAmmV2Pool {
    pub pool: Pubkey,
    pub token_x_vault: Pubkey,
    pub token_sol_vault: Pubkey,
    pub token_mint: Pubkey,
    pub base_mint: Pubkey,
}

#[derive(Debug, Clone)]
pub struct VertigoPool {
    pub pool: Pubkey,
    pub pool_owner: Pubkey,
    pub token_x_vault: Pubkey,
    pub token_sol_vault: Pubkey,
    pub token_mint: Pubkey,
    pub base_mint: Pubkey,
}

#[derive(Debug, Clone)]
pub struct HeavenPool {
    pub pool: Pubkey,
    pub protocol_config: Pubkey,
    pub token_x_vault: Pubkey,
    pub token_base_vault: Pubkey,
    pub token_mint: Pubkey,
    pub base_mint: Pubkey,
    pub token_program: Pubkey, // Support for Token-2022
}

#[derive(Debug, Clone)]
pub struct MintPoolData {
    pub mint: Pubkey,
    pub token_program: Pubkey, // Support for both Token and Token 2022
    pub wallet_account: Pubkey,
    pub wallet_wsol_account: Pubkey,
    pub raydium_pools: Vec<RaydiumPool>,
    pub raydium_cp_pools: Vec<RaydiumCpPool>,
    pub pump_pools: Vec<PumpPool>,
    pub dlmm_pairs: Vec<DlmmPool>,
    pub whirlpool_pools: Vec<WhirlpoolPool>,
    pub raydium_clmm_pools: Vec<RaydiumClmmPool>,
    pub meteora_damm_pools: Vec<MeteoraDAmmPool>,
    pub meteora_damm_v2_pools: Vec<MeteoraDAmmV2Pool>,
    pub vertigo_pools: Vec<VertigoPool>,
    pub heaven_pools: Vec<HeavenPool>,
}

impl MintPoolData {
    pub fn new(mint: Pubkey, wallet_account: &Pubkey, token_program: Pubkey) -> Self {
        let sol = sol_mint();
        let wallet_wsol_pk =
            spl_associated_token_account::get_associated_token_address(wallet_account, &sol);
        Self {
            mint,
            token_program,
            wallet_account: *wallet_account,
            wallet_wsol_account: wallet_wsol_pk,
            raydium_pools: Vec::new(),
            raydium_cp_pools: Vec::new(),
            pump_pools: Vec::new(),
            dlmm_pairs: Vec::new(),
            whirlpool_pools: Vec::new(),
            raydium_clmm_pools: Vec::new(),
            meteora_damm_pools: Vec::new(),
            meteora_damm_v2_pools: Vec::new(),
            vertigo_pools: Vec::new(),
            heaven_pools: Vec::new(),
        }
    }

    pub fn add_raydium_pool(
        &mut self,
        pool: Pubkey,
        token_vault: Pubkey,
        sol_vault: Pubkey,
        token_mint: Pubkey,
        base_mint: Pubkey,
    ) {
        self.raydium_pools.push(RaydiumPool {
            pool,
            token_vault,
            sol_vault,
            token_mint,
            base_mint,
        });
    }

    pub fn add_raydium_cp_pool(
        &mut self,
        pool: Pubkey,
        token_vault: Pubkey,
        sol_vault: Pubkey,
        amm_config: Pubkey,
        observation: Pubkey,
        token_mint: Pubkey,
        base_mint: Pubkey,
    ) {
        self.raydium_cp_pools.push(RaydiumCpPool {
            pool,
            token_vault,
            sol_vault,
            amm_config,
            observation,
            token_mint,
            base_mint,
        });
    }

    pub fn add_pump_pool(
        &mut self,
        pool: Pubkey,
        token_vault: Pubkey,
        sol_vault: Pubkey,
        fee_wallet: Pubkey,
        fee_token_wallet: Pubkey,
        coin_creator_vault_ata: Pubkey,
        coin_creator_vault_authority: Pubkey,
        token_mint: Pubkey,
        base_mint: Pubkey,
        is_mayhem_mode: bool,
    ) {
        self.pump_pools.push(PumpPool {
            pool,
            token_vault,
            sol_vault,
            fee_wallet,
            fee_token_wallet,
            coin_creator_vault_ata,
            coin_creator_vault_authority,
            token_mint,
            base_mint,
            is_mayhem_mode,
        });
    }

    pub fn add_dlmm_pool(
        &mut self,
        pair: Pubkey,
        token_vault: Pubkey,
        sol_vault: Pubkey,
        oracle: Pubkey,
        bin_arrays: Vec<Pubkey>,
        memo_program: Option<Pubkey>,
        token_mint: Pubkey,
        base_mint: Pubkey,
    ) {
        self.dlmm_pairs.push(DlmmPool {
            pair,
            token_vault,
            sol_vault,
            oracle,
            bin_arrays,
            memo_program,
            token_mint,
            base_mint,
        });
    }

    pub fn add_whirlpool_pool(
        &mut self,
        pool: Pubkey,
        oracle: Pubkey,
        x_vault: Pubkey,
        y_vault: Pubkey,
        tick_arrays: Vec<Pubkey>,
        memo_program: Option<Pubkey>,
        token_mint: Pubkey,
        base_mint: Pubkey,
    ) {
        self.whirlpool_pools.push(WhirlpoolPool {
            pool,
            oracle,
            x_vault,
            y_vault,
            tick_arrays,
            memo_program,
            token_mint,
            base_mint,
        });
    }

    pub fn add_raydium_clmm_pool(
        &mut self,
        pool: Pubkey,
        amm_config: Pubkey,
        observation_state: Pubkey,
        x_vault: Pubkey,
        y_vault: Pubkey,
        tick_arrays: Vec<Pubkey>,
        memo_program: Option<Pubkey>,
        token_mint: Pubkey,
        base_mint: Pubkey,
    ) {
        let bitmap_extension = Pubkey::find_program_address(
            &[
                POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
                pool.as_ref(),
            ],
            &raydium_clmm_program_id(),
        )
        .0;

        self.raydium_clmm_pools.push(RaydiumClmmPool {
            pool,
            amm_config,
            observation_state,
            x_vault,
            y_vault,
            bitmap_extension,
            tick_arrays,
            memo_program,
            token_mint,
            base_mint,
        });
    }

    pub fn add_meteora_damm_pool(
        &mut self,
        pool: Pubkey,
        token_x_vault: Pubkey,
        token_sol_vault: Pubkey,
        token_x_token_vault: Pubkey,
        token_sol_token_vault: Pubkey,
        token_x_lp_mint: Pubkey,
        token_sol_lp_mint: Pubkey,
        token_x_pool_lp: Pubkey,
        token_sol_pool_lp: Pubkey,
        admin_token_fee_x: Pubkey,
        admin_token_fee_sol: Pubkey,
        token_mint: Pubkey,
        base_mint: Pubkey,
    ) {
        self.meteora_damm_pools.push(MeteoraDAmmPool {
            pool,
            token_x_vault,
            token_sol_vault,
            token_x_token_vault,
            token_sol_token_vault,
            token_x_lp_mint,
            token_sol_lp_mint,
            token_x_pool_lp,
            token_sol_pool_lp,
            admin_token_fee_x,
            admin_token_fee_sol,
            token_mint,
            base_mint,
        });
    }

    pub fn add_meteora_damm_v2_pool(
        &mut self,
        pool: Pubkey,
        token_x_vault: Pubkey,
        token_sol_vault: Pubkey,
        token_mint: Pubkey,
        base_mint: Pubkey,
    ) {
        self.meteora_damm_v2_pools.push(MeteoraDAmmV2Pool {
            pool,
            token_x_vault,
            token_sol_vault,
            token_mint,
            base_mint,
        });
    }

    pub fn add_vertigo_pool(
        &mut self,
        pool: Pubkey,
        pool_owner: Pubkey,
        token_x_vault: Pubkey,
        token_sol_vault: Pubkey,
        token_mint: Pubkey,
        base_mint: Pubkey,
    ) {
        self.vertigo_pools.push(VertigoPool {
            pool,
            pool_owner,
            token_x_vault,
            token_sol_vault,
            token_mint,
            base_mint,
        });
    }

    pub fn add_heaven_pool(
        &mut self,
        pool: Pubkey,
        protocol_config: Pubkey,
        token_x_vault: Pubkey,
        token_base_vault: Pubkey,
        token_mint: Pubkey,
        base_mint: Pubkey,
        token_program: Pubkey,
    ) {
        self.heaven_pools.push(HeavenPool {
            pool,
            protocol_config,
            token_x_vault,
            token_base_vault,
            token_mint,
            base_mint,
            token_program,
        });
    }
}
