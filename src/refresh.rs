use crate::config::MarketsConfig;
use crate::constants::sol_mint;
use crate::dex::heaven::{heaven_program_id, HeavenPoolState};
use crate::dex::meteora::constants::{damm_program_id, damm_v2_program_id};
use crate::dex::meteora::dammv2_info::MeteoraDAmmV2Info;
use crate::dex::meteora::{constants::dlmm_program_id, dlmm_info::DlmmInfo};
use crate::dex::pump::{pump_fee_wallet, pump_mayhem_fee_wallet, pump_program_id, PumpAmmInfo};
use crate::dex::raydium::{
    get_tick_array_pubkeys, raydium_clmm_program_id, raydium_cp_program_id, raydium_program_id,
    PoolState, RaydiumAmmInfo, RaydiumCpAmmInfo,
};
use crate::dex::vertigo::{derive_vault_address, vertigo_program_id, VertigoInfo};
use crate::dex::whirlpool::{
    constants::whirlpool_program_id, state::Whirlpool, update_tick_array_accounts_for_onchain,
};
use crate::pools::*;
use solana_client::rpc_client::RpcClient;
use solana_program::pubkey::Pubkey;
use spl_associated_token_account;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Enum representing the different DEX pool types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarketPoolKind {
    Pump,
    RaydiumV4,
    RaydiumCp,
    RaydiumClmm,
    MeteoraDlmm,
    MeteoraDamm,
    MeteoraDammV2,
    Whirlpool,
    Vertigo,
    Heaven,
}

/// Internal structure for grouping pools by mint during detection
#[derive(Default)]
struct MintPoolsBuilder {
    pump_pools: Vec<String>,
    raydium_pools: Vec<String>,
    raydium_cp_pools: Vec<String>,
    raydium_clmm_pools: Vec<String>,
    dlmm_pools: Vec<String>,
    damm_pools: Vec<String>,
    damm_v2_pools: Vec<String>,
    whirlpool_pools: Vec<String>,
    vertigo_pools: Vec<String>,
    heaven_pools: Vec<String>,
}

/// Detect the pool kind based on the account owner (program ID)
pub fn detect_pool_kind(owner: &Pubkey) -> Option<MarketPoolKind> {
    if *owner == pump_program_id() {
        Some(MarketPoolKind::Pump)
    } else if *owner == raydium_program_id() {
        Some(MarketPoolKind::RaydiumV4)
    } else if *owner == raydium_cp_program_id() {
        Some(MarketPoolKind::RaydiumCp)
    } else if *owner == raydium_clmm_program_id() {
        Some(MarketPoolKind::RaydiumClmm)
    } else if *owner == dlmm_program_id() {
        Some(MarketPoolKind::MeteoraDlmm)
    } else if *owner == damm_program_id() {
        Some(MarketPoolKind::MeteoraDamm)
    } else if *owner == damm_v2_program_id() {
        Some(MarketPoolKind::MeteoraDammV2)
    } else if *owner == whirlpool_program_id() {
        Some(MarketPoolKind::Whirlpool)
    } else if *owner == vertigo_program_id() {
        Some(MarketPoolKind::Vertigo)
    } else if *owner == heaven_program_id() {
        Some(MarketPoolKind::Heaven)
    } else {
        None
    }
}

/// Extract the non-SOL token mint from a pool based on its kind
fn extract_token_mint(
    kind: MarketPoolKind,
    data: &[u8],
    pool_pubkey: &Pubkey,
) -> anyhow::Result<Option<Pubkey>> {
    let sol = sol_mint();

    match kind {
        MarketPoolKind::Pump => {
            let info = PumpAmmInfo::load_checked(data)?;
            let token_mint = if info.base_mint == sol {
                info.quote_mint
            } else if info.quote_mint == sol {
                info.base_mint
            } else {
                return Ok(None); // Neither side is SOL
            };
            Ok(Some(token_mint))
        }
        MarketPoolKind::RaydiumV4 => {
            let info = RaydiumAmmInfo::load_checked(data)?;
            let token_mint = if info.coin_mint == sol {
                info.pc_mint
            } else if info.pc_mint == sol {
                info.coin_mint
            } else {
                return Ok(None);
            };
            Ok(Some(token_mint))
        }
        MarketPoolKind::RaydiumCp => {
            let info = RaydiumCpAmmInfo::load_checked(data)?;
            let token_mint = if info.token_0_mint == sol {
                info.token_1_mint
            } else if info.token_1_mint == sol {
                info.token_0_mint
            } else {
                return Ok(None);
            };
            Ok(Some(token_mint))
        }
        MarketPoolKind::RaydiumClmm => {
            let info = PoolState::load_checked(data)?;
            let token_mint = if info.token_mint_0 == sol {
                info.token_mint_1
            } else if info.token_mint_1 == sol {
                info.token_mint_0
            } else {
                return Ok(None);
            };
            Ok(Some(token_mint))
        }
        MarketPoolKind::MeteoraDlmm => {
            let info = DlmmInfo::load_checked(data)?;
            let token_mint = if info.token_x_mint == sol {
                info.token_y_mint
            } else if info.token_y_mint == sol {
                info.token_x_mint
            } else {
                return Ok(None);
            };
            Ok(Some(token_mint))
        }
        MarketPoolKind::MeteoraDamm => {
            let pool = meteora_damm_cpi::Pool::deserialize_unchecked(data)?;
            let token_mint = if pool.token_a_mint == sol {
                pool.token_b_mint
            } else if pool.token_b_mint == sol {
                pool.token_a_mint
            } else {
                return Ok(None);
            };
            Ok(Some(token_mint))
        }
        MarketPoolKind::MeteoraDammV2 => {
            let info = MeteoraDAmmV2Info::load_checked(data)?;
            let token_mint = if info.base_mint == sol {
                info.quote_mint
            } else if info.quote_mint == sol {
                info.base_mint
            } else {
                return Ok(None);
            };
            Ok(Some(token_mint))
        }
        MarketPoolKind::Whirlpool => {
            let whirlpool = Whirlpool::try_deserialize(data)?;
            let token_mint = if whirlpool.token_mint_a == sol {
                whirlpool.token_mint_b
            } else if whirlpool.token_mint_b == sol {
                whirlpool.token_mint_a
            } else {
                return Ok(None);
            };
            Ok(Some(token_mint))
        }
        MarketPoolKind::Vertigo => {
            let info = VertigoInfo::load_checked(data, pool_pubkey)?;
            let token_mint = if info.mint_a == sol {
                info.mint_b
            } else if info.mint_b == sol {
                info.mint_a
            } else {
                return Ok(None);
            };
            Ok(Some(token_mint))
        }
        MarketPoolKind::Heaven => {
            let info = HeavenPoolState::parse(data).ok_or_else(|| {
                anyhow::anyhow!("Failed to parse Heaven pool")
            })?;
            let usdc_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
            let token_mint = if info.mint_a == sol || info.mint_a == usdc_mint {
                info.mint_b
            } else if info.mint_b == sol || info.mint_b == usdc_mint {
                info.mint_a
            } else {
                return Ok(None);
            };
            Ok(Some(token_mint))
        }
    }
}

/// Initialize pools from a simplified markets config
/// This function:
/// 1. Fetches all market accounts
/// 2. Detects the pool kind for each
/// 3. Extracts the token mint
/// 4. Groups pools by mint
/// 5. Initializes MintPoolData for each mint
pub async fn initialize_pools_from_markets(
    markets_config: &MarketsConfig,
    wallet_account: &str,
    rpc_client: Arc<RpcClient>,
) -> anyhow::Result<HashMap<Pubkey, MintPoolData>> {
    info!("Initializing pools from {} markets", markets_config.markets.len());

    // Parse all market addresses
    let market_pubkeys: Vec<Pubkey> = markets_config
        .markets
        .iter()
        .filter_map(|s| {
            match Pubkey::from_str(s) {
                Ok(pk) => Some(pk),
                Err(e) => {
                    error!("Invalid market address {}: {}", s, e);
                    None
                }
            }
        })
        .collect();

    if market_pubkeys.is_empty() {
        return Ok(HashMap::new());
    }

    // Fetch all accounts in batches
    let mut mint_pools: HashMap<Pubkey, MintPoolsBuilder> = HashMap::new();

    // Process in batches of 100 (RPC limit for getMultipleAccounts)
    for chunk in market_pubkeys.chunks(100) {
        let accounts = rpc_client.get_multiple_accounts(chunk)?;

        for (i, maybe_account) in accounts.iter().enumerate() {
            let pool_pubkey = chunk[i];

            let account = match maybe_account {
                Some(acc) => acc,
                None => {
                    warn!("Market account {} not found", pool_pubkey);
                    continue;
                }
            };

            // Detect pool kind
            let kind = match detect_pool_kind(&account.owner) {
                Some(k) => k,
                None => {
                    warn!(
                        "Unknown pool program {} for market {}",
                        account.owner, pool_pubkey
                    );
                    continue;
                }
            };

            info!("Detected {:?} pool: {}", kind, pool_pubkey);

            // Extract token mint
            let token_mint = match extract_token_mint(kind, &account.data, &pool_pubkey) {
                Ok(Some(mint)) => mint,
                Ok(None) => {
                    warn!("Pool {} does not have SOL as one side, skipping", pool_pubkey);
                    continue;
                }
                Err(e) => {
                    error!("Failed to parse pool {}: {}", pool_pubkey, e);
                    continue;
                }
            };

            info!("  Token mint: {}", token_mint);

            // Group by mint
            let builder = mint_pools.entry(token_mint).or_default();
            let pool_str = pool_pubkey.to_string();

            match kind {
                MarketPoolKind::Pump => builder.pump_pools.push(pool_str),
                MarketPoolKind::RaydiumV4 => builder.raydium_pools.push(pool_str),
                MarketPoolKind::RaydiumCp => builder.raydium_cp_pools.push(pool_str),
                MarketPoolKind::RaydiumClmm => builder.raydium_clmm_pools.push(pool_str),
                MarketPoolKind::MeteoraDlmm => builder.dlmm_pools.push(pool_str),
                MarketPoolKind::MeteoraDamm => builder.damm_pools.push(pool_str),
                MarketPoolKind::MeteoraDammV2 => builder.damm_v2_pools.push(pool_str),
                MarketPoolKind::Whirlpool => builder.whirlpool_pools.push(pool_str),
                MarketPoolKind::Vertigo => builder.vertigo_pools.push(pool_str),
                MarketPoolKind::Heaven => builder.heaven_pools.push(pool_str),
            }
        }
    }

    info!("Found {} unique token mints", mint_pools.len());

    // Initialize MintPoolData for each mint
    let mut result: HashMap<Pubkey, MintPoolData> = HashMap::new();

    for (mint, builder) in mint_pools {
        info!("Initializing pools for mint: {}", mint);

        let pool_data = initialize_pool_data(
            &mint.to_string(),
            wallet_account,
            if builder.raydium_pools.is_empty() { None } else { Some(&builder.raydium_pools) },
            if builder.raydium_cp_pools.is_empty() { None } else { Some(&builder.raydium_cp_pools) },
            if builder.pump_pools.is_empty() { None } else { Some(&builder.pump_pools) },
            if builder.dlmm_pools.is_empty() { None } else { Some(&builder.dlmm_pools) },
            if builder.whirlpool_pools.is_empty() { None } else { Some(&builder.whirlpool_pools) },
            if builder.raydium_clmm_pools.is_empty() { None } else { Some(&builder.raydium_clmm_pools) },
            if builder.damm_pools.is_empty() { None } else { Some(&builder.damm_pools) },
            if builder.damm_v2_pools.is_empty() { None } else { Some(&builder.damm_v2_pools) },
            if builder.vertigo_pools.is_empty() { None } else { Some(&builder.vertigo_pools) },
            if builder.heaven_pools.is_empty() { None } else { Some(&builder.heaven_pools) },
            rpc_client.clone(),
        )
        .await?;

        result.insert(mint, pool_data);
    }

    Ok(result)
}

pub async fn initialize_pool_data(
    mint: &str,
    wallet_account: &str,
    raydium_pools: Option<&Vec<String>>,
    raydium_cp_pools: Option<&Vec<String>>,
    pump_pools: Option<&Vec<String>>,
    dlmm_pools: Option<&Vec<String>>,
    whirlpool_pools: Option<&Vec<String>>,
    raydium_clmm_pools: Option<&Vec<String>>,
    meteora_damm_pools: Option<&Vec<String>>,
    meteora_damm_v2_pools: Option<&Vec<String>>,
    vertigo_pools: Option<&Vec<String>>,
    heaven_pools: Option<&Vec<String>>,
    rpc_client: Arc<RpcClient>,
) -> anyhow::Result<MintPoolData> {
    info!("Initializing pool data for mint: {}", mint);

    // Fetch mint account to determine token program
    let mint_pubkey = Pubkey::from_str(mint)?;
    let mint_account = rpc_client.get_account(&mint_pubkey)?;

    // Determine token program based on mint account owner
    let token_2022_program_id =
        Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap();
    let token_program = if mint_account.owner == spl_token::ID {
        spl_token::ID
    } else if mint_account.owner == token_2022_program_id {
        token_2022_program_id
    } else {
        return Err(anyhow::anyhow!("Unknown token program for mint: {}", mint));
    };

    info!("Detected token program: {}", token_program);
    let mut pool_data = MintPoolData::new(mint, wallet_account, token_program)?;
    info!("Pool data initialized for mint: {}", mint);

    if let Some(pools) = pump_pools {
        for pool_address in pools {
            let pump_pool_pubkey = Pubkey::from_str(pool_address)?;

            match rpc_client.get_account(&pump_pool_pubkey) {
                Ok(account) => {
                    if account.owner != pump_program_id() {
                        error!(
                            "Error: Pump pool account is not owned by the Pump program. Expected: {}, Actual: {}",
                            pump_program_id(), account.owner
                        );
                        return Err(anyhow::anyhow!(
                            "Pump pool account is not owned by the Pump program"
                        ));
                    }

                    match PumpAmmInfo::load_checked(&account.data) {
                        Ok(amm_info) => {
                            let (sol_vault, token_vault) = if sol_mint() == amm_info.base_mint {
                                (
                                    amm_info.pool_base_token_account,
                                    amm_info.pool_quote_token_account,
                                )
                            } else if sol_mint() == amm_info.quote_mint {
                                (
                                    amm_info.pool_quote_token_account,
                                    amm_info.pool_base_token_account,
                                )
                            } else {
                                (
                                    amm_info.pool_quote_token_account,
                                    amm_info.pool_base_token_account,
                                )
                            };

                            let (fee_wallet, fee_token_wallet) =
                                if amm_info.is_mayhem_mode {
                                    let wallet = pump_mayhem_fee_wallet();
                                    (
                                        wallet,
                                        spl_associated_token_account::get_associated_token_address(
                                            &wallet,
                                            &amm_info.quote_mint,
                                        ),
                                    )
                                } else {
                                    let wallet = pump_fee_wallet();
                                    (
                                        wallet,
                                        spl_associated_token_account::get_associated_token_address(
                                            &wallet,
                                            &amm_info.quote_mint,
                                        ),
                                    )
                                };

                            let coin_creator_vault_ata =
                                spl_associated_token_account::get_associated_token_address(
                                    &amm_info.coin_creator_vault_authority,
                                    &amm_info.quote_mint,
                                );

                            // Determine token_mint and base_mint
                            let (token_mint, base_mint) = if mint_pubkey == amm_info.base_mint {
                                (amm_info.base_mint, amm_info.quote_mint)
                            } else {
                                (amm_info.quote_mint, amm_info.base_mint)
                            };
                            
                            pool_data.add_pump_pool(
                                pool_address,
                                &token_vault.to_string(),
                                &sol_vault.to_string(),
                                &fee_wallet.to_string(),
                                &fee_token_wallet.to_string(),
                                &coin_creator_vault_ata.to_string(),
                                &amm_info.coin_creator_vault_authority.to_string(),
                                &token_mint.to_string(),
                                &base_mint.to_string(),
                                amm_info.is_mayhem_mode,
                            )?;
                            info!("Pump pool added: {}", pool_address);
                            info!("    Base mint: {}", amm_info.base_mint.to_string());
                            info!("    Quote mint: {}", amm_info.quote_mint.to_string());
                            info!("    Token vault: {}", token_vault.to_string());
                            info!("    Sol vault: {}", sol_vault.to_string());
                            info!("    Fee wallet: {}", fee_wallet.to_string());
                            info!("    Fee token wallet: {}", fee_token_wallet.to_string());
                            info!(
                                "    Coin creator vault ata: {}",
                                coin_creator_vault_ata.to_string()
                            );
                            info!(
                                "    Coin creator vault authority: {}",
                                amm_info.coin_creator_vault_authority.to_string()
                            );
                            info!("    Mayhem mode: {}", amm_info.is_mayhem_mode);
                            info!("    Initialized Pump pool: {}\n", pump_pool_pubkey);
                        }
                        Err(e) => {
                            error!(
                                "Error parsing AmmInfo from Pump pool {}: {:?}",
                                pump_pool_pubkey, e
                            );
                            return Err(e);
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Error fetching Pump pool account {}: {:?}",
                        pump_pool_pubkey, e
                    );
                    return Err(anyhow::anyhow!("Error fetching Pump pool account"));
                }
            }
        }
    }

    if let Some(pools) = raydium_pools {
        for pool_address in pools {
            let raydium_pool_pubkey = Pubkey::from_str(pool_address)?;

            match rpc_client.get_account(&raydium_pool_pubkey) {
                Ok(account) => {
                    if account.owner != raydium_program_id() {
                        error!(
                            "Error: Raydium pool account is not owned by the Raydium program. Expected: {}, Actual: {}",
                            raydium_program_id(), account.owner
                        );
                        return Err(anyhow::anyhow!(
                            "Raydium pool account is not owned by the Raydium program"
                        ));
                    }

                    match RaydiumAmmInfo::load_checked(&account.data) {
                        Ok(amm_info) => {
                            if amm_info.coin_mint != pool_data.mint
                                && amm_info.pc_mint != pool_data.mint
                            {
                                error!(
                                    "Mint {} is not present in Raydium pool {}, skipping",
                                    pool_data.mint, raydium_pool_pubkey
                                );
                                return Err(anyhow::anyhow!(
                                    "Invalid Raydium pool: {}",
                                    raydium_pool_pubkey
                                ));
                            }

                            if amm_info.coin_mint != sol_mint() && amm_info.pc_mint != sol_mint() {
                                error!(
                                    "SOL is not present in Raydium pool {}",
                                    raydium_pool_pubkey
                                );
                                return Err(anyhow::anyhow!(
                                    "SOL is not present in Raydium pool: {}",
                                    raydium_pool_pubkey
                                ));
                            }

                            let (sol_vault, token_vault) = if sol_mint() == amm_info.coin_mint {
                                (amm_info.coin_vault, amm_info.pc_vault)
                            } else {
                                (amm_info.pc_vault, amm_info.coin_vault)
                            };

                            // Determine token_mint and base_mint
                            let (token_mint, base_mint) = if mint_pubkey == amm_info.coin_mint {
                                (amm_info.coin_mint, amm_info.pc_mint)
                            } else {
                                (amm_info.pc_mint, amm_info.coin_mint)
                            };
                            
                            pool_data.add_raydium_pool(
                                pool_address,
                                &token_vault.to_string(),
                                &sol_vault.to_string(),
                                &token_mint.to_string(),
                                &base_mint.to_string(),
                            )?;
                            info!("Raydium pool added: {}", pool_address);
                            info!("    Coin mint: {}", amm_info.coin_mint.to_string());
                            info!("    PC mint: {}", amm_info.pc_mint.to_string());
                            info!("    Token vault: {}", token_vault.to_string());
                            info!("    Sol vault: {}", sol_vault.to_string());
                            info!("    Initialized Raydium pool: {}\n", raydium_pool_pubkey);
                        }
                        Err(e) => {
                            error!(
                                "Error parsing AmmInfo from Raydium pool {}: {:?}",
                                raydium_pool_pubkey, e
                            );
                            return Err(e);
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Error fetching Raydium pool account {}: {:?}",
                        raydium_pool_pubkey, e
                    );
                    return Err(anyhow::anyhow!("Error fetching Raydium pool account"));
                }
            }
        }
    }

    if let Some(pools) = raydium_cp_pools {
        for pool_address in pools {
            let raydium_cp_pool_pubkey = Pubkey::from_str(pool_address)?;

            match rpc_client.get_account(&raydium_cp_pool_pubkey) {
                Ok(account) => {
                    if account.owner != raydium_cp_program_id() {
                        error!(
                            "Error: Raydium CP pool account is not owned by the Raydium CP program. Expected: {}, Actual: {}",
                            raydium_cp_program_id(), account.owner
                        );
                        return Err(anyhow::anyhow!(
                            "Raydium CP pool account is not owned by the Raydium CP program"
                        ));
                    }

                    match RaydiumCpAmmInfo::load_checked(&account.data) {
                        Ok(amm_info) => {
                            if amm_info.token_0_mint != pool_data.mint
                                && amm_info.token_1_mint != pool_data.mint
                            {
                                error!(
                                    "Mint {} is not present in Raydium CP pool {}, skipping",
                                    pool_data.mint, raydium_cp_pool_pubkey
                                );
                                return Err(anyhow::anyhow!(
                                    "Invalid Raydium CP pool: {}",
                                    raydium_cp_pool_pubkey
                                ));
                            }

                            let (sol_vault, token_vault) = if sol_mint() == amm_info.token_0_mint {
                                (amm_info.token_0_vault, amm_info.token_1_vault)
                            } else if sol_mint() == amm_info.token_1_mint {
                                (amm_info.token_1_vault, amm_info.token_0_vault)
                            } else {
                                error!(
                                    "SOL is not present in Raydium CP pool {}",
                                    raydium_cp_pool_pubkey
                                );
                                return Err(anyhow::anyhow!(
                                    "SOL is not present in Raydium CP pool: {}",
                                    raydium_cp_pool_pubkey
                                ));
                            };

                            // Determine token_mint and base_mint
                            let (token_mint, base_mint) = if mint_pubkey == amm_info.token_0_mint {
                                (amm_info.token_0_mint, amm_info.token_1_mint)
                            } else {
                                (amm_info.token_1_mint, amm_info.token_0_mint)
                            };
                            
                            pool_data.add_raydium_cp_pool(
                                pool_address,
                                &token_vault.to_string(),
                                &sol_vault.to_string(),
                                &amm_info.amm_config.to_string(),
                                &amm_info.observation_key.to_string(),
                                &token_mint.to_string(),
                                &base_mint.to_string(),
                            )?;
                            info!("Raydium CP pool added: {}", pool_address);
                            info!("    Token vault: {}", token_vault.to_string());
                            info!("    Sol vault: {}", sol_vault.to_string());
                            info!("    AMM Config: {}", amm_info.amm_config.to_string());
                            info!(
                                "    Observation Key: {}\n",
                                amm_info.observation_key.to_string()
                            );
                        }
                        Err(e) => {
                            error!(
                                "Error parsing AmmInfo from Raydium CP pool {}: {:?}",
                                raydium_cp_pool_pubkey, e
                            );
                            return Err(e);
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Error fetching Raydium CP pool account {}: {:?}",
                        raydium_cp_pool_pubkey, e
                    );
                    return Err(anyhow::anyhow!("Error fetching Raydium CP pool account"));
                }
            }
        }
    }
    if let Some(pools) = dlmm_pools {
        for pool_address in pools {
            let dlmm_pool_pubkey = Pubkey::from_str(pool_address)?;

            match rpc_client.get_account(&dlmm_pool_pubkey) {
                Ok(account) => {
                    if account.owner != dlmm_program_id() {
                        error!(
                            "Error: DLMM pool account is not owned by the DLMM program. Expected: {}, Actual: {}",
                            dlmm_program_id(), account.owner
                        );
                        return Err(anyhow::anyhow!(
                            "DLMM pool account is not owned by the DLMM program"
                        ));
                    }

                    match DlmmInfo::load_checked(&account.data) {
                        Ok(amm_info) => {
                            let sol_mint = sol_mint();
                            let (token_vault, sol_vault) =
                                amm_info.get_token_and_sol_vaults(&pool_data.mint, &sol_mint);

                            let bin_arrays = match amm_info.calculate_bin_arrays(&dlmm_pool_pubkey)
                            {
                                Ok(arrays) => arrays,
                                Err(e) => {
                                    error!(
                                        "Error calculating bin arrays for DLMM pool {}: {:?}",
                                        dlmm_pool_pubkey, e
                                    );
                                    return Err(e);
                                }
                            };

                            let bin_array_strings: Vec<String> =
                                bin_arrays.iter().map(|pubkey| pubkey.to_string()).collect();
                            let bin_array_str_refs: Vec<&str> =
                                bin_array_strings.iter().map(|s| s.as_str()).collect();

                            // Determine token_mint and base_mint
                            let (token_mint, base_mint) = if mint_pubkey == amm_info.token_x_mint {
                                (amm_info.token_x_mint, amm_info.token_y_mint)
                            } else {
                                (amm_info.token_y_mint, amm_info.token_x_mint)
                            };
                            
                            pool_data.add_dlmm_pool(
                                pool_address,
                                &token_vault.to_string(),
                                &sol_vault.to_string(),
                                &amm_info.oracle.to_string(),
                                bin_array_str_refs,
                                None, // memo_program
                                &token_mint.to_string(),
                                &base_mint.to_string(),
                            )?;

                            info!("DLMM pool added: {}", pool_address);
                            info!("    Token X Mint: {}", amm_info.token_x_mint.to_string());
                            info!("    Token Y Mint: {}", amm_info.token_y_mint.to_string());
                            info!("    Token vault: {}", token_vault.to_string());
                            info!("    Sol vault: {}", sol_vault.to_string());
                            info!("    Oracle: {}", amm_info.oracle.to_string());
                            info!("    Active ID: {}", amm_info.active_id);

                            for (i, array) in bin_array_strings.iter().enumerate() {
                                info!("    Bin Array {}: {}", i, array);
                            }
                            info!("");
                        }
                        Err(e) => {
                            error!(
                                "Error parsing AmmInfo from DLMM pool {}: {:?}",
                                dlmm_pool_pubkey, e
                            );
                            return Err(e);
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Error fetching DLMM pool account {}: {:?}",
                        dlmm_pool_pubkey, e
                    );
                    return Err(anyhow::anyhow!("Error fetching DLMM pool account"));
                }
            }
        }
    }

    if let Some(pools) = whirlpool_pools {
        for pool_address in pools {
            let whirlpool_pool_pubkey = Pubkey::from_str(pool_address)?;

            match rpc_client.get_account(&whirlpool_pool_pubkey) {
                Ok(account) => {
                    if account.owner != whirlpool_program_id() {
                        error!(
                            "Error: Whirlpool pool account is not owned by the Whirlpool program. Expected: {}, Actual: {}",
                            whirlpool_program_id(), account.owner
                        );
                        return Err(anyhow::anyhow!(
                            "Whirlpool pool account is not owned by the Whirlpool program"
                        ));
                    }

                    match Whirlpool::try_deserialize(&account.data) {
                        Ok(whirlpool) => {
                            if whirlpool.token_mint_a != pool_data.mint
                                && whirlpool.token_mint_b != pool_data.mint
                            {
                                error!(
                                    "Mint {} is not present in Whirlpool pool {}, skipping",
                                    pool_data.mint, whirlpool_pool_pubkey
                                );
                                return Err(anyhow::anyhow!(
                                    "Invalid Whirlpool pool: {}",
                                    whirlpool_pool_pubkey
                                ));
                            }

                            let sol_mint = sol_mint();
                            let (sol_vault, token_vault) = if sol_mint == whirlpool.token_mint_a {
                                (whirlpool.token_vault_a, whirlpool.token_vault_b)
                            } else if sol_mint == whirlpool.token_mint_b {
                                (whirlpool.token_vault_b, whirlpool.token_vault_a)
                            } else {
                                error!(
                                    "SOL is not present in Whirlpool pool {}",
                                    whirlpool_pool_pubkey
                                );
                                return Err(anyhow::anyhow!(
                                    "SOL is not present in Whirlpool pool: {}",
                                    whirlpool_pool_pubkey
                                ));
                            };

                            let whirlpool_oracle = Pubkey::find_program_address(
                                &[b"oracle", whirlpool_pool_pubkey.as_ref()],
                                &whirlpool_program_id(),
                            )
                            .0;

                            let whirlpool_tick_arrays = update_tick_array_accounts_for_onchain(
                                &whirlpool,
                                &whirlpool_pool_pubkey,
                                &whirlpool_program_id(),
                            );

                            let tick_array_strings: Vec<String> = whirlpool_tick_arrays
                                .iter()
                                .map(|meta| meta.pubkey.to_string())
                                .collect();

                            let tick_array_str_refs: Vec<&str> =
                                tick_array_strings.iter().map(|s| s.as_str()).collect();

                            // Determine token_mint and base_mint
                            let (token_mint, base_mint) = if mint_pubkey == whirlpool.token_mint_a {
                                (whirlpool.token_mint_a, whirlpool.token_mint_b)
                            } else {
                                (whirlpool.token_mint_b, whirlpool.token_mint_a)
                            };
                            
                            pool_data.add_whirlpool_pool(
                                pool_address,
                                &whirlpool_oracle.to_string(),
                                &token_vault.to_string(),
                                &sol_vault.to_string(),
                                tick_array_str_refs,
                                None, // memo_program
                                &token_mint.to_string(),
                                &base_mint.to_string(),
                            )?;

                            info!("Whirlpool pool added: {}", pool_address);
                            info!("    Token mint A: {}", whirlpool.token_mint_a.to_string());
                            info!("    Token mint B: {}", whirlpool.token_mint_b.to_string());
                            info!("    Token vault: {}", token_vault.to_string());
                            info!("    Sol vault: {}", sol_vault.to_string());
                            info!("    Oracle: {}", whirlpool_oracle.to_string());

                            for (i, array) in tick_array_strings.iter().enumerate() {
                                info!("    Tick Array {}: {}", i, array);
                            }
                            info!("");
                        }
                        Err(e) => {
                            error!(
                                "Error parsing Whirlpool data from pool {}: {:?}",
                                whirlpool_pool_pubkey, e
                            );
                            return Err(anyhow::anyhow!("Error parsing Whirlpool data"));
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Error fetching Whirlpool pool account {}: {:?}",
                        whirlpool_pool_pubkey, e
                    );
                    return Err(anyhow::anyhow!("Error fetching Whirlpool pool account"));
                }
            }
        }
    }

    if let Some(pools) = raydium_clmm_pools {
        for pool_address in pools {
            let raydium_clmm_program_id = raydium_clmm_program_id();

            match rpc_client.get_account(&Pubkey::from_str(pool_address)?) {
                Ok(account) => {
                    if account.owner != raydium_clmm_program_id {
                        error!(
                            "Raydium CLMM pool {} is not owned by the Raydium CLMM program, skipping",
                            pool_address
                        );
                        continue;
                    }

                    match PoolState::load_checked(&account.data) {
                        Ok(raydium_clmm) => {
                            if raydium_clmm.token_mint_0 != pool_data.mint
                                && raydium_clmm.token_mint_1 != pool_data.mint
                            {
                                error!(
                                    "Mint {} is not present in Raydium CLMM pool {}, skipping",
                                    pool_data.mint, pool_address
                                );
                                continue;
                            }

                            let sol_mint = sol_mint();
                            let (token_vault, sol_vault) = if sol_mint == raydium_clmm.token_mint_0
                            {
                                (raydium_clmm.token_vault_1, raydium_clmm.token_vault_0)
                            } else if sol_mint == raydium_clmm.token_mint_1 {
                                (raydium_clmm.token_vault_0, raydium_clmm.token_vault_1)
                            } else {
                                error!("SOL is not present in Raydium CLMM pool {}", pool_address);
                                continue;
                            };

                            let tick_array_pubkeys = get_tick_array_pubkeys(
                                &Pubkey::from_str(pool_address)?,
                                raydium_clmm.tick_current,
                                raydium_clmm.tick_spacing,
                                &[-1, 0, 1],
                                &raydium_clmm_program_id,
                            )?;

                            let tick_array_strings: Vec<String> = tick_array_pubkeys
                                .iter()
                                .map(|pubkey| pubkey.to_string())
                                .collect();

                            let tick_array_str_refs: Vec<&str> =
                                tick_array_strings.iter().map(|s| s.as_str()).collect();

                            // Determine token_mint and base_mint
                            let (token_mint, base_mint) = if mint_pubkey == raydium_clmm.token_mint_0 {
                                (raydium_clmm.token_mint_0, raydium_clmm.token_mint_1)
                            } else {
                                (raydium_clmm.token_mint_1, raydium_clmm.token_mint_0)
                            };
                            
                            pool_data.add_raydium_clmm_pool(
                                pool_address,
                                &raydium_clmm.amm_config.to_string(),
                                &raydium_clmm.observation_key.to_string(),
                                &token_vault.to_string(),
                                &sol_vault.to_string(),
                                tick_array_str_refs,
                                None, // memo_program
                                &token_mint.to_string(),
                                &base_mint.to_string(),
                            )?;

                            info!("Raydium CLMM pool added: {}", pool_address);
                            info!(
                                "    Token mint 0: {}",
                                raydium_clmm.token_mint_0.to_string()
                            );
                            info!(
                                "    Token mint 1: {}",
                                raydium_clmm.token_mint_1.to_string()
                            );
                            info!("    Token vault: {}", token_vault.to_string());
                            info!("    Sol vault: {}", sol_vault.to_string());
                            info!("    AMM config: {}", raydium_clmm.amm_config.to_string());
                            info!(
                                "    Observation key: {}",
                                raydium_clmm.observation_key.to_string()
                            );

                            for (i, array) in tick_array_strings.iter().enumerate() {
                                info!("    Tick Array {}: {}", i, array);
                            }
                            info!("");
                        }
                        Err(e) => {
                            error!(
                                "Error parsing Raydium CLMM data from pool {}: {:?}",
                                pool_address, e
                            );
                            continue;
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Error fetching Raydium CLMM pool account {}: {:?}",
                        pool_address, e
                    );
                    continue;
                }
            }
        }
    }

    if let Some(pools) = meteora_damm_pools {
        for pool_address in pools {
            let meteora_damm_pool_pubkey = Pubkey::from_str(pool_address)?;

            match rpc_client.get_account(&meteora_damm_pool_pubkey) {
                Ok(account) => {
                    if account.owner != damm_program_id() {
                        error!(
                            "Error: Meteora DAMM pool account is not owned by the Meteora DAMM program. Expected: {}, Actual: {}",
                            damm_program_id(), account.owner
                        );
                        return Err(anyhow::anyhow!(
                            "Meteora DAMM pool account is not owned by the Meteora DAMM program"
                        ));
                    }

                    match meteora_damm_cpi::Pool::deserialize_unchecked(&account.data) {
                        Ok(pool) => {
                            if pool.token_a_mint != pool_data.mint
                                && pool.token_b_mint != pool_data.mint
                            {
                                error!(
                                    "Mint {} is not present in Meteora DAMM pool {}, skipping",
                                    pool_data.mint, meteora_damm_pool_pubkey
                                );
                                return Err(anyhow::anyhow!(
                                    "Invalid Meteora DAMM pool: {}",
                                    meteora_damm_pool_pubkey
                                ));
                            }

                            let sol_mint = sol_mint();
                            if pool.token_a_mint != sol_mint && pool.token_b_mint != sol_mint {
                                error!(
                                    "SOL is not present in Meteora DAMM pool {}",
                                    meteora_damm_pool_pubkey
                                );
                                return Err(anyhow::anyhow!(
                                    "SOL is not present in Meteora DAMM pool: {}",
                                    meteora_damm_pool_pubkey
                                ));
                            }

                            let (x_vault, sol_vault) = if sol_mint == pool.token_a_mint {
                                (pool.b_vault, pool.a_vault)
                            } else {
                                (pool.a_vault, pool.b_vault)
                            };

                            // Fetch vault accounts
                            let x_vault_data = rpc_client.get_account(&x_vault)?;
                            let sol_vault_data = rpc_client.get_account(&sol_vault)?;

                            let x_vault_obj = meteora_vault_cpi::Vault::deserialize_unchecked(
                                &mut x_vault_data.data.as_slice(),
                            )?;
                            let sol_vault_obj = meteora_vault_cpi::Vault::deserialize_unchecked(
                                &mut sol_vault_data.data.as_slice(),
                            )?;

                            let x_token_vault = x_vault_obj.token_vault;
                            let sol_token_vault = sol_vault_obj.token_vault;
                            let x_lp_mint = x_vault_obj.lp_mint;
                            let sol_lp_mint = sol_vault_obj.lp_mint;

                            let (x_pool_lp, sol_pool_lp) = if sol_mint == pool.token_a_mint {
                                (pool.b_vault_lp, pool.a_vault_lp)
                            } else {
                                (pool.a_vault_lp, pool.b_vault_lp)
                            };

                            let (x_admin_fee, sol_admin_fee) = if sol_mint == pool.token_a_mint {
                                (pool.admin_token_b_fee, pool.admin_token_a_fee)
                            } else {
                                (pool.admin_token_a_fee, pool.admin_token_b_fee)
                            };

                            // Determine token_mint and base_mint
                            let (token_mint, base_mint) = if mint_pubkey == pool.token_a_mint {
                                (pool.token_a_mint, pool.token_b_mint)
                            } else {
                                (pool.token_b_mint, pool.token_a_mint)
                            };
                            
                            pool_data.add_meteora_damm_pool(
                                pool_address,
                                &x_vault.to_string(),
                                &sol_vault.to_string(),
                                &x_token_vault.to_string(),
                                &sol_token_vault.to_string(),
                                &x_lp_mint.to_string(),
                                &sol_lp_mint.to_string(),
                                &x_pool_lp.to_string(),
                                &sol_pool_lp.to_string(),
                                &x_admin_fee.to_string(),
                                &sol_admin_fee.to_string(),
                                &token_mint.to_string(),
                                &base_mint.to_string(),
                            )?;

                            info!("Meteora DAMM pool added: {}", pool_address);
                            info!("    Token X vault: {}", x_token_vault.to_string());
                            info!("    SOL vault: {}", sol_token_vault.to_string());
                            info!("    Token X LP mint: {}", x_lp_mint.to_string());
                            info!("    SOL LP mint: {}", sol_lp_mint.to_string());
                            info!("    Token X pool LP: {}", x_pool_lp.to_string());
                            info!("    SOL pool LP: {}", sol_pool_lp.to_string());
                            info!("    Token X admin fee: {}", x_admin_fee.to_string());
                            info!("    SOL admin fee: {}", sol_admin_fee.to_string());
                            info!("");
                        }
                        Err(e) => {
                            error!(
                                "Error parsing Meteora DAMM pool data from pool {}: {:?}",
                                meteora_damm_pool_pubkey, e
                            );
                            return Err(anyhow::anyhow!("Error parsing Meteora DAMM pool data"));
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Error fetching Meteora DAMM pool account {}: {:?}",
                        meteora_damm_pool_pubkey, e
                    );
                    return Err(anyhow::anyhow!("Error fetching Meteora DAMM pool account"));
                }
            }
        }
    }

    if let Some(pools) = meteora_damm_v2_pools {
        for pool_address in pools {
            let meteora_damm_v2_pool_pubkey = Pubkey::from_str(pool_address)?;

            match rpc_client.get_account(&meteora_damm_v2_pool_pubkey) {
                Ok(account) => {
                    if account.owner != damm_v2_program_id() {
                        error!("Meteora DAMM V2 pool {} is not owned by the Meteora DAMM V2 program, skipping", pool_address);
                        continue;
                    }

                    match MeteoraDAmmV2Info::load_checked(&account.data) {
                        Ok(meteora_damm_v2_info) => {
                            info!("Meteora DAMM V2 pool added: {}", pool_address);
                            info!(
                                "    Base mint: {}",
                                meteora_damm_v2_info.base_mint.to_string()
                            );
                            info!(
                                "    Quote mint: {}",
                                meteora_damm_v2_info.quote_mint.to_string()
                            );
                            info!(
                                "    Base vault: {}",
                                meteora_damm_v2_info.base_vault.to_string()
                            );
                            info!(
                                "    Quote vault: {}",
                                meteora_damm_v2_info.quote_vault.to_string()
                            );
                            info!("");
                            let token_x_vault = if sol_mint() == meteora_damm_v2_info.base_mint {
                                meteora_damm_v2_info.quote_vault
                            } else {
                                meteora_damm_v2_info.base_vault
                            };

                            let token_sol_vault = if sol_mint() == meteora_damm_v2_info.base_mint {
                                meteora_damm_v2_info.base_vault
                            } else {
                                meteora_damm_v2_info.quote_vault
                            };
                            // Determine token_mint and base_mint
                            let (token_mint, base_mint) = if mint_pubkey == meteora_damm_v2_info.base_mint {
                                (meteora_damm_v2_info.base_mint, meteora_damm_v2_info.quote_mint)
                            } else {
                                (meteora_damm_v2_info.quote_mint, meteora_damm_v2_info.base_mint)
                            };
                            
                            pool_data.add_meteora_damm_v2_pool(
                                pool_address,
                                &token_x_vault.to_string(),
                                &token_sol_vault.to_string(),
                                &token_mint.to_string(),
                                &base_mint.to_string(),
                            )?;
                        }
                        Err(e) => {
                            error!(
                                "Error parsing Meteora DAMM V2 pool data from pool {}: {:?}",
                                meteora_damm_v2_pool_pubkey, e
                            );
                            continue;
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Error fetching Meteora DAMM V2 pool account {}: {:?}",
                        meteora_damm_v2_pool_pubkey, e
                    );
                    continue;
                }
            }
        }
    }

    if let Some(pools) = vertigo_pools {
        for pool_address in pools {
            let vertigo_pool_pubkey = Pubkey::from_str(pool_address)?;

            match rpc_client.get_account(&vertigo_pool_pubkey) {
                Ok(account) => {
                    if account.owner != vertigo_program_id() {
                        error!(
                            "Error: Vertigo pool account is not owned by the Vertigo program. Expected: {}, Actual: {}",
                            vertigo_program_id(), account.owner
                        );
                        return Err(anyhow::anyhow!(
                            "Vertigo pool account is not owned by the Vertigo program"
                        ));
                    }

                    match VertigoInfo::load_checked(&account.data, &vertigo_pool_pubkey) {
                        Ok(vertigo_info) => {
                            info!("Vertigo pool added: {}", pool_address);
                            info!("    Mint A: {}", vertigo_info.mint_a.to_string());
                            info!("    Mint B: {}", vertigo_info.mint_b.to_string());

                            let base_mint = pool_data.mint.to_string();

                            // Following the original loading pattern from user's code:
                            let non_base_vault = if base_mint == vertigo_info.mint_a.to_string() {
                                derive_vault_address(&vertigo_pool_pubkey, &vertigo_info.mint_b).0
                            } else {
                                derive_vault_address(&vertigo_pool_pubkey, &vertigo_info.mint_a).0
                            };
                            let base_vault = if base_mint == vertigo_info.mint_a.to_string() {
                                derive_vault_address(&vertigo_pool_pubkey, &vertigo_info.mint_a).0
                            } else {
                                derive_vault_address(&vertigo_pool_pubkey, &vertigo_info.mint_b).0
                            };

                            // Map to transaction expected fields:
                            // base_mint is our trading token, non-base should be SOL
                            let token_x_vault = base_vault; // vault for our trading token
                            let token_sol_vault = non_base_vault; // vault for SOL

                            info!("    Token X Vault: {}", token_x_vault.to_string());
                            info!("    Token SOL Vault: {}", token_sol_vault.to_string());
                            info!("");

                            // Determine token_mint and base_mint
                            let (token_mint, base_mint) = if mint_pubkey == vertigo_info.mint_a {
                                (vertigo_info.mint_a, vertigo_info.mint_b)
                            } else {
                                (vertigo_info.mint_b, vertigo_info.mint_a)
                            };
                            
                            pool_data.add_vertigo_pool(
                                pool_address,
                                &vertigo_info.pool.to_string(),
                                &token_x_vault.to_string(),
                                &token_sol_vault.to_string(),
                                &token_mint.to_string(),
                                &base_mint.to_string(),
                            )?;
                        }
                        Err(e) => {
                            error!(
                                "Error parsing Vertigo pool data from pool {}: {:?}",
                                vertigo_pool_pubkey, e
                            );
                            continue;
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Error fetching Vertigo pool account {}: {:?}",
                        vertigo_pool_pubkey, e
                    );
                    continue;
                }
            }
        }
    }

    if let Some(pools) = heaven_pools {
        for pool_address in pools {
            let heaven_pool_pubkey = Pubkey::from_str(pool_address)?;

            match rpc_client.get_account(&heaven_pool_pubkey) {
                Ok(account) => {
                    if account.owner != heaven_program_id() {
                        error!(
                            "Error: Heaven pool account is not owned by the Heaven program. Expected: {}, Actual: {}",
                            heaven_program_id(), account.owner
                        );
                        return Err(anyhow::anyhow!(
                            "Heaven pool account is not owned by the Heaven program"
                        ));
                    }

                    match HeavenPoolState::parse(&account.data) {
                        Some(heaven_info) => {
                            info!("Heaven pool added: {}", pool_address);
                            info!("    Mint A: {}", heaven_info.mint_a.to_string());
                            info!("    Mint B: {}", heaven_info.mint_b.to_string());
                            info!("    Vault A: {}", heaven_info.vault_a.to_string());
                            info!("    Vault B: {}", heaven_info.vault_b.to_string());
                            info!("    Protocol Config: {}", heaven_info.protocol_config.to_string());
                            info!("    Reserve A: {}", heaven_info.reserve_a);
                            info!("    Reserve B: {}", heaven_info.reserve_b);

                            // Determine which vault corresponds to token and base
                            let (token_x_vault, token_base_vault) = 
                                if mint_pubkey == heaven_info.mint_a {
                                    (heaven_info.vault_a, heaven_info.vault_b)
                                } else {
                                    (heaven_info.vault_b, heaven_info.vault_a)
                                };

                            // Determine token_mint and base_mint
                            let (token_mint, base_mint) = if mint_pubkey == heaven_info.mint_a {
                                (heaven_info.mint_a, heaven_info.mint_b)
                            } else {
                                (heaven_info.mint_b, heaven_info.mint_a)
                            };

                            // Validate that the base mint is either SOL or USDC
                            let usdc_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
                            if base_mint != sol_mint() && base_mint != usdc_mint {
                                error!(
                                    "Invalid Heaven pool: Expected SOL or USDC as base mint, but found {}",
                                    base_mint
                                );
                                return Err(anyhow::anyhow!(
                                    "Invalid Heaven pool: Expected SOL or USDC as base mint"
                                ));
                            }

                            // Determine token program - check if either mint is Token-2022
                            let token_2022_program_id =
                                Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap();
                            
                            // For now, use the same token program as the mint
                            // TODO: Check both mints to determine if Token-2022 is needed
                            let token_program = token_program;

                            pool_data.add_heaven_pool(
                                pool_address,
                                &heaven_info.protocol_config.to_string(),
                                &token_x_vault.to_string(),
                                &token_base_vault.to_string(),
                                &token_mint.to_string(),
                                &base_mint.to_string(),
                                &token_program.to_string(),
                            )?;
                            
                            info!("    Initialized Heaven pool: {}\n", heaven_pool_pubkey);
                        }
                        None => {
                            error!(
                                "Error parsing Heaven pool data from pool {}",
                                heaven_pool_pubkey
                            );
                            return Err(anyhow::anyhow!("Failed to parse Heaven pool data"));
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Error fetching Heaven pool account {}: {:?}",
                        heaven_pool_pubkey, e
                    );
                    continue;
                }
            }
        }
    }

    Ok(pool_data)
}
