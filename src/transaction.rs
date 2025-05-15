use crate::config::Config;
use crate::dex::raydium::{raydium_authority, raydium_cp_authority};
use crate::kamino::{
    get_kamino_flashloan_borrow_ix, get_kamino_flashloan_repay_ix, KAMINO_ADDITIONAL_COMPUTE_UNITS,
};
use crate::pools::MintPoolData;
use solana_client::rpc_client::RpcClient;
use solana_program::instruction::Instruction;
use solana_sdk::address_lookup_table::AddressLookupTableAccount;
use solana_sdk::commitment_config::CommitmentLevel;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::hash::Hash;
use solana_sdk::message::v0::Message;
use solana_sdk::signature::{Keypair, Signature};
use solana_sdk::signer::Signer;
use solana_sdk::transaction::VersionedTransaction;
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::constants::sol_mint;
use crate::dex::meteora::constants::{
    damm_program_id, dlmm_event_authority, dlmm_program_id, vault_program_id,
};
use crate::dex::pump::constants::{pump_fee_wallet, pump_program_id};
use crate::dex::raydium::constants::{
    raydium_clmm_program_id, raydium_cp_program_id, raydium_program_id,
};
use crate::dex::whirlpool::constants::whirlpool_program_id;
use solana_program::instruction::AccountMeta;
use solana_program::system_program;
use solana_program::{pubkey, pubkey::Pubkey};
use spl_associated_token_account::ID as associated_token_program_id;
use spl_token::ID as token_program_id;

const EXECUTOR_PROGRAM_ID: Pubkey = pubkey!("MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz");
const FEE_COLLECTOR: Pubkey = pubkey!("6AGB9kqgSp2mQXwYpdrV4QVV8urvCaDS35U1wsLssy6H");
const PUMP_FUN_GLOBAL_CONFIG: Pubkey = pubkey!("ADyA8hdefvWN2dbGGWFotbzWxrAvLW83WG6QCVXvJKqw");
const PUMP_AUTHORITY: Pubkey = pubkey!("GS4CU59F31iL7aR2Q8zVS8DRrcRnXX1yjQ66TqNVQnaR");

pub async fn build_and_send_transaction(
    wallet_kp: &Keypair,
    config: &Config,
    mint_pool_data: &MintPoolData,
    rpc_clients: &[Arc<RpcClient>],
    blockhash: Hash,
    address_lookup_table_accounts: &[AddressLookupTableAccount],
) -> anyhow::Result<Vec<Signature>> {
    let enable_kamino = config
        .kamino_flashloan
        .as_ref()
        .map_or(false, |k| k.enabled);
    let compute_unit_limit = config.bot.compute_unit_limit
        + if enable_kamino {
            KAMINO_ADDITIONAL_COMPUTE_UNITS
        } else {
            0
        };

    let mut instructions = vec![];
    // Add a random number here to make each transaction unique
    let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(
        compute_unit_limit + rand::random::<u32>() % 1000,
    );
    instructions.push(compute_budget_ix);

    let compute_unit_price = config.spam.as_ref().map_or(1000, |s| s.compute_unit_price);
    let compute_budget_price_ix =
        ComputeBudgetInstruction::set_compute_unit_price(compute_unit_price);
    instructions.push(compute_budget_price_ix);

    let swap_ix = create_swap_instruction(wallet_kp, mint_pool_data)?;

    let mut all_instructions = instructions.clone();
    if enable_kamino {
        debug!("Adding Kamino flashloan borrow instruction");
        let borrow_ix = get_kamino_flashloan_borrow_ix(
            &wallet_kp.pubkey(),
            mint_pool_data.wallet_wsol_account,
        )?;
        all_instructions.push(borrow_ix);
    }

    debug!("Adding swap instruction");
    all_instructions.push(swap_ix);

    if enable_kamino {
        debug!("Adding Kamino flashloan repay instruction");
        let repay_ix = get_kamino_flashloan_repay_ix(
            &wallet_kp.pubkey(),
            mint_pool_data.wallet_wsol_account,
            2, // Borrow instruction index
        )?;
        all_instructions.push(repay_ix);
    }

    let message = Message::try_compile(
        &wallet_kp.pubkey(),
        &all_instructions,
        address_lookup_table_accounts,
        blockhash,
    )?;

    let tx = VersionedTransaction::try_new(
        solana_sdk::message::VersionedMessage::V0(message),
        &[wallet_kp],
    )?;

    let max_retries = config
        .spam
        .as_ref()
        .and_then(|s| s.max_retries)
        .unwrap_or(3);

    let mut signatures = Vec::new();

    for (i, client) in rpc_clients.iter().enumerate() {
        debug!("Sending transaction through RPC client {}", i);

        let signature = match send_transaction_with_retries(client, &tx, max_retries).await {
            Ok(sig) => sig,
            Err(e) => {
                error!("Failed to send transaction through RPC client {}: {}", i, e);
                continue;
            }
        };

        info!(
            "Transaction sent successfully through RPC client {}: {}",
            i, signature
        );
        signatures.push(signature);
    }

    Ok(signatures)
}

async fn send_transaction_with_retries(
    client: &RpcClient,
    tx: &VersionedTransaction,
    max_retries: u64,
) -> anyhow::Result<Signature> {
    Ok(client.send_transaction_with_config(
        tx,
        solana_client::rpc_config::RpcSendTransactionConfig {
            skip_preflight: true,
            max_retries: Some(max_retries as usize),
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            ..Default::default()
        },
    )?)
}

// See https://docs.solanamevbot.com/home/onchain-bot/onchain-program for more information
fn create_swap_instruction(
    wallet_kp: &Keypair,
    mint_pool_data: &MintPoolData,
) -> anyhow::Result<Instruction> {
    debug!("Creating swap instruction for all DEX types");

    let wallet = wallet_kp.pubkey();
    let sol_mint_pubkey = sol_mint();
    let wallet_sol_account = mint_pool_data.wallet_wsol_account;

    let mut accounts = vec![
        AccountMeta::new_readonly(wallet, true), // 0. Wallet (signer)
        AccountMeta::new_readonly(sol_mint_pubkey, false), // 1. SOL mint
        AccountMeta::new(FEE_COLLECTOR, false),  // 2. Fee collector
        AccountMeta::new(wallet_sol_account, false), // 3. Wallet SOL account
        AccountMeta::new_readonly(token_program_id, false), // 4. Token program
        AccountMeta::new_readonly(system_program::ID, false), // 5. System program
        AccountMeta::new_readonly(associated_token_program_id, false), // 6. Associated Token program
    ];

    accounts.push(AccountMeta::new_readonly(mint_pool_data.mint, false));
    let wallet_x_account =
        spl_associated_token_account::get_associated_token_address(&wallet, &mint_pool_data.mint);
    accounts.push(AccountMeta::new(wallet_x_account, false));

    for pool in &mint_pool_data.raydium_pools {
        accounts.push(AccountMeta::new_readonly(raydium_program_id(), false));
        accounts.push(AccountMeta::new_readonly(raydium_authority(), false)); // Raydium authority
        accounts.push(AccountMeta::new(pool.pool, false));
        accounts.push(AccountMeta::new(pool.token_vault, false));
        accounts.push(AccountMeta::new(pool.sol_vault, false));
    }

    for pool in &mint_pool_data.raydium_cp_pools {
        accounts.push(AccountMeta::new_readonly(raydium_cp_program_id(), false));
        accounts.push(AccountMeta::new_readonly(raydium_cp_authority(), false)); // Raydium CP authority
        accounts.push(AccountMeta::new(pool.pool, false));
        accounts.push(AccountMeta::new_readonly(pool.amm_config, false));
        accounts.push(AccountMeta::new(pool.token_vault, false));
        accounts.push(AccountMeta::new(pool.sol_vault, false));
        accounts.push(AccountMeta::new(pool.observation, false));
    }

    for pool in &mint_pool_data.pump_pools {
        accounts.push(AccountMeta::new_readonly(pump_program_id(), false));
        accounts.push(AccountMeta::new_readonly(PUMP_FUN_GLOBAL_CONFIG, false));
        accounts.push(AccountMeta::new_readonly(PUMP_AUTHORITY, false));
        accounts.push(AccountMeta::new_readonly(pump_fee_wallet(), false));
        accounts.push(AccountMeta::new_readonly(pool.pool, false));
        accounts.push(AccountMeta::new(pool.token_vault, false));
        accounts.push(AccountMeta::new(pool.sol_vault, false));
        accounts.push(AccountMeta::new(pool.fee_token_wallet, false));
        accounts.push(AccountMeta::new(pool.coin_creator_vault_ata, false));
        accounts.push(AccountMeta::new_readonly(
            pool.coin_creator_vault_authority,
            false,
        ));
    }

    for pair in &mint_pool_data.dlmm_pairs {
        accounts.push(AccountMeta::new_readonly(dlmm_program_id(), false));
        accounts.push(AccountMeta::new(dlmm_event_authority(), false)); // DLMM event authority
        accounts.push(AccountMeta::new(pair.pair, false));
        accounts.push(AccountMeta::new(pair.token_vault, false));
        accounts.push(AccountMeta::new(pair.sol_vault, false));
        accounts.push(AccountMeta::new(pair.oracle, false));
        for bin_array in &pair.bin_arrays {
            accounts.push(AccountMeta::new(*bin_array, false));
        }
    }

    for pool in &mint_pool_data.whirlpool_pools {
        accounts.push(AccountMeta::new_readonly(whirlpool_program_id(), false));
        accounts.push(AccountMeta::new(pool.pool, false));
        accounts.push(AccountMeta::new(pool.oracle, false));
        accounts.push(AccountMeta::new(pool.x_vault, false));
        accounts.push(AccountMeta::new(pool.y_vault, false));
        for tick_array in &pool.tick_arrays {
            accounts.push(AccountMeta::new(*tick_array, false));
        }
    }

    for pool in &mint_pool_data.raydium_clmm_pools {
        accounts.push(AccountMeta::new_readonly(raydium_clmm_program_id(), false));
        accounts.push(AccountMeta::new(pool.pool, false));
        accounts.push(AccountMeta::new_readonly(pool.amm_config, false));
        accounts.push(AccountMeta::new(pool.observation_state, false));
        accounts.push(AccountMeta::new(pool.bitmap_extension, false));
        accounts.push(AccountMeta::new(pool.x_vault, false));
        accounts.push(AccountMeta::new(pool.y_vault, false));
        for tick_array in &pool.tick_arrays {
            accounts.push(AccountMeta::new(*tick_array, false));
        }
    }

    for pool in &mint_pool_data.meteora_damm_pools {
        accounts.push(AccountMeta::new_readonly(damm_program_id(), false));
        accounts.push(AccountMeta::new_readonly(vault_program_id(), false));
        accounts.push(AccountMeta::new(pool.pool, false));
        accounts.push(AccountMeta::new(pool.token_x_vault, false));
        accounts.push(AccountMeta::new(pool.token_sol_vault, false));
        accounts.push(AccountMeta::new(pool.token_x_token_vault, false));
        accounts.push(AccountMeta::new(pool.token_sol_token_vault, false));
        accounts.push(AccountMeta::new(pool.token_x_lp_mint, false));
        accounts.push(AccountMeta::new(pool.token_sol_lp_mint, false));
        accounts.push(AccountMeta::new(pool.token_x_pool_lp, false));
        accounts.push(AccountMeta::new(pool.token_sol_pool_lp, false));
        accounts.push(AccountMeta::new(pool.admin_token_fee_x, false));
        accounts.push(AccountMeta::new(pool.admin_token_fee_sol, false));
    }

    let mut data = vec![15u8];

    let minimum_profit: u64 = 0;
    /*
        max_bin_to_process is how many bins the program can look through when calculating optimal trade size.
        Each bin lookup will take ~10k compute units. So you should setup your compute unit limit accordingly.
        For example, I usually use 650_000 compute unit limit with max_bin_to_process = 30.
        The full bot uses the following code to calculate this number:

        let bins = ((config.bot.compute_unit_limit - 300_000 - if enable_kamino { 80_000 } else { 0 })
            / 10_000) as u64;
        Here we just use a default value of 20 for demonstration purposes.
    */
    let max_bin_to_process: u64 = 20;
    // When true, the bot will not fail the transaction even when it can't find a profitable arbitrage. It will just do nothing and succeed.
    let no_failure_mode = false;

    data.extend_from_slice(&minimum_profit.to_le_bytes());
    data.extend_from_slice(&max_bin_to_process.to_le_bytes());
    data.extend_from_slice(if no_failure_mode { &[1] } else { &[0] });

    Ok(Instruction {
        program_id: EXECUTOR_PROGRAM_ID,
        accounts,
        data,
    })
}
