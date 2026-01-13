use crate::dex::byreal::byreal_program_id;
use crate::dex::meteora::dlmm_info::DlmmInfo;
use crate::dex::pancakeswap::pancakeswap_program_id;
use crate::dex::raydium::{get_tick_array_pubkeys, raydium_clmm_program_id, PoolState};
use crate::dex::whirlpool::constants::whirlpool_program_id;
use crate::dex::whirlpool::state::Whirlpool;
use crate::dex::whirlpool::update_tick_array_accounts_for_onchain;
use crate::pools::MintPoolData;
use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_program::pubkey::Pubkey;
use tracing::{info, warn};

/// Program IDs for CLMM pools
pub struct ProgramIds {
    pub whirlpool: Pubkey,
    pub raydium_clmm: Pubkey,
    pub pancakeswap: Pubkey,
    pub byreal: Pubkey,
}

impl ProgramIds {
    pub fn new() -> Self {
        Self {
            whirlpool: whirlpool_program_id(),
            raydium_clmm: raydium_clmm_program_id(),
            pancakeswap: pancakeswap_program_id(),
            byreal: byreal_program_id(),
        }
    }
}

impl Default for ProgramIds {
    fn default() -> Self {
        Self::new()
    }
}

/// Refresh DLMM pools by recalculating bin arrays based on current active_id
pub fn refresh_dlmm_pools(
    pool_data: &mut MintPoolData,
    rpc_client: &RpcClient,
    suppress_logs: bool,
) -> Result<()> {
    for pool in pool_data.dlmm_pairs.iter_mut() {
        match rpc_client.get_account(&pool.pair) {
            Ok(account) => {
                match DlmmInfo::load_checked(&account.data) {
                    Ok(dlmm_info) => {
                        match dlmm_info.calculate_bin_arrays(&pool.pair) {
                            Ok(new_bin_arrays) => {
                                pool.bin_arrays = new_bin_arrays;
                                if !suppress_logs {
                                    info!(
                                        "DLMM pool {} bin arrays refreshed, active_id: {}",
                                        pool.pair, dlmm_info.active_id
                                    );
                                }
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to calculate bin arrays for DLMM pool {}: {}",
                                    pool.pair, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse DLMM pool {}: {}", pool.pair, e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to fetch DLMM pool {}: {}", pool.pair, e);
            }
        }
    }
    Ok(())
}

/// Refresh Whirlpool pools by recalculating tick arrays based on current tick
pub fn refresh_whirlpool_pools(
    pool_data: &mut MintPoolData,
    rpc_client: &RpcClient,
    program_id: &Pubkey,
    suppress_logs: bool,
) -> Result<()> {
    for pool in pool_data.whirlpool_pools.iter_mut() {
        match rpc_client.get_account(&pool.pool) {
            Ok(account) => {
                match Whirlpool::try_deserialize(&account.data) {
                    Ok(whirlpool) => {
                        let tick_array_metas = update_tick_array_accounts_for_onchain(
                            &whirlpool,
                            &pool.pool,
                            program_id,
                        );
                        pool.tick_arrays = tick_array_metas.iter().map(|m| m.pubkey).collect();
                        if !suppress_logs {
                            info!(
                                "Whirlpool {} tick arrays refreshed at tick {}",
                                pool.pool, whirlpool.tick_current_index
                            );
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse Whirlpool {}: {}", pool.pool, e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to fetch Whirlpool pool {}: {}", pool.pool, e);
            }
        }
    }
    Ok(())
}

/// Refresh Raydium CLMM pools by recalculating tick arrays based on current tick
pub fn refresh_raydium_clmm_pools(
    pool_data: &mut MintPoolData,
    rpc_client: &RpcClient,
    program_id: &Pubkey,
    suppress_logs: bool,
) -> Result<()> {
    for pool in pool_data.raydium_clmm_pools.iter_mut() {
        match rpc_client.get_account(&pool.pool) {
            Ok(account) => {
                if account.owner != *program_id {
                    warn!(
                        "Raydium CLMM pool {} owner mismatch (expected {}, got {})",
                        pool.pool, program_id, account.owner
                    );
                    continue;
                }

                match PoolState::load_checked(&account.data) {
                    Ok(pool_state) => {
                        match get_tick_array_pubkeys(
                            &pool.pool,
                            pool_state.tick_current,
                            pool_state.tick_spacing,
                            &[-1, 0, 1],
                            program_id,
                        ) {
                            Ok(tick_arrays) => {
                                pool.tick_arrays = tick_arrays;
                                if !suppress_logs {
                                    info!(
                                        "Raydium CLMM {} tick arrays refreshed at tick {}",
                                        pool.pool, pool_state.tick_current
                                    );
                                }
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to calculate tick arrays for Raydium CLMM pool {}: {}",
                                    pool.pool, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse Raydium CLMM pool {}: {}", pool.pool, e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to fetch Raydium CLMM pool {}: {}", pool.pool, e);
            }
        }
    }
    Ok(())
}

/// Refresh PancakeSwap pools by recalculating tick arrays based on current tick
/// PancakeSwap uses the same CLMM layout as Raydium
pub fn refresh_pancakeswap_pools(
    pool_data: &mut MintPoolData,
    rpc_client: &RpcClient,
    program_id: &Pubkey,
    suppress_logs: bool,
) -> Result<()> {
    for pool in pool_data.pancakeswap_pools.iter_mut() {
        match rpc_client.get_account(&pool.pool) {
            Ok(account) => {
                if account.owner != *program_id {
                    warn!(
                        "PancakeSwap pool {} owner mismatch (expected {}, got {})",
                        pool.pool, program_id, account.owner
                    );
                    continue;
                }

                match PoolState::load_checked(&account.data) {
                    Ok(pool_state) => {
                        match get_tick_array_pubkeys(
                            &pool.pool,
                            pool_state.tick_current,
                            pool_state.tick_spacing,
                            &[-1, 0, 1],
                            program_id,
                        ) {
                            Ok(tick_arrays) => {
                                pool.tick_arrays = tick_arrays;
                                if !suppress_logs {
                                    info!(
                                        "PancakeSwap {} tick arrays refreshed at tick {}",
                                        pool.pool, pool_state.tick_current
                                    );
                                }
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to calculate tick arrays for PancakeSwap pool {}: {}",
                                    pool.pool, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse PancakeSwap pool {}: {}", pool.pool, e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to fetch PancakeSwap pool {}: {}", pool.pool, e);
            }
        }
    }
    Ok(())
}

/// Refresh Byreal pools by recalculating tick arrays based on current tick
/// Byreal uses the same CLMM layout as Raydium
pub fn refresh_byreal_pools(
    pool_data: &mut MintPoolData,
    rpc_client: &RpcClient,
    program_id: &Pubkey,
    suppress_logs: bool,
) -> Result<()> {
    for pool in pool_data.byreal_pools.iter_mut() {
        match rpc_client.get_account(&pool.pool) {
            Ok(account) => {
                if account.owner != *program_id {
                    warn!(
                        "Byreal pool {} owner mismatch (expected {}, got {})",
                        pool.pool, program_id, account.owner
                    );
                    continue;
                }

                match PoolState::load_checked(&account.data) {
                    Ok(pool_state) => {
                        match get_tick_array_pubkeys(
                            &pool.pool,
                            pool_state.tick_current,
                            pool_state.tick_spacing,
                            &[-1, 0, 1],
                            program_id,
                        ) {
                            Ok(tick_arrays) => {
                                pool.tick_arrays = tick_arrays;
                                if !suppress_logs {
                                    info!(
                                        "Byreal {} tick arrays refreshed at tick {}",
                                        pool.pool, pool_state.tick_current
                                    );
                                }
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to calculate tick arrays for Byreal pool {}: {}",
                                    pool.pool, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse Byreal pool {}: {}", pool.pool, e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to fetch Byreal pool {}: {}", pool.pool, e);
            }
        }
    }
    Ok(())
}

/// Pool data refresher that orchestrates all DEX-specific refreshers
pub struct PoolDataRefresher {
    pub program_ids: ProgramIds,
}

impl PoolDataRefresher {
    pub fn new() -> Self {
        Self {
            program_ids: ProgramIds::new(),
        }
    }

    /// Refresh all CLMM pool bin/tick arrays based on current pool state
    pub fn refresh_all_pools(
        &self,
        pool_data: &mut MintPoolData,
        rpc_client: &RpcClient,
        suppress_logs: bool,
    ) -> Result<()> {
        // Refresh DLMM pools (Meteora)
        if !pool_data.dlmm_pairs.is_empty() {
            refresh_dlmm_pools(pool_data, rpc_client, suppress_logs)?;
        }

        // Refresh Whirlpool pools (Orca)
        if !pool_data.whirlpool_pools.is_empty() {
            refresh_whirlpool_pools(
                pool_data,
                rpc_client,
                &self.program_ids.whirlpool,
                suppress_logs,
            )?;
        }

        // Refresh Raydium CLMM pools
        if !pool_data.raydium_clmm_pools.is_empty() {
            refresh_raydium_clmm_pools(
                pool_data,
                rpc_client,
                &self.program_ids.raydium_clmm,
                suppress_logs,
            )?;
        }

        // Refresh PancakeSwap pools
        if !pool_data.pancakeswap_pools.is_empty() {
            refresh_pancakeswap_pools(
                pool_data,
                rpc_client,
                &self.program_ids.pancakeswap,
                suppress_logs,
            )?;
        }

        // Refresh Byreal pools
        if !pool_data.byreal_pools.is_empty() {
            refresh_byreal_pools(
                pool_data,
                rpc_client,
                &self.program_ids.byreal,
                suppress_logs,
            )?;
        }

        Ok(())
    }
}

impl Default for PoolDataRefresher {
    fn default() -> Self {
        Self::new()
    }
}
