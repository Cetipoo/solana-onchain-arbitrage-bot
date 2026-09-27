//! The V10 transaction for a basket, sent as a v1 transaction to every
//! configured RPC.
use crate::route::Basket;
use anyhow::{ensure, Result};
use executor_v10_abi::MAX_COMPUTE_UNIT_LIMIT;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_commitment_config::CommitmentLevel;
use solana_sdk::{
    hash::Hash,
    message::{
        v1::{self, MAX_ADDRESSES, MAX_TRANSACTION_SIZE, SIGNATURE_SIZE},
        VersionedMessage,
    },
    signer::Signer,
    transaction::VersionedTransaction,
};
use tracing::{error, info};

/// CU the transaction's limit keeps above the executor's own allowance.
const EXECUTOR_CU_MARGIN: u32 = 300;
const LOADED_ACCOUNTS_DATA_SIZE_LIMIT: u32 = 64 * 1024 * 1024;

/// The basket's transaction. The program's own limits leave room for more
/// pools than a v1 transaction can carry, so an oversized basket is a
/// configuration error.
pub fn build_transaction(
    payer: &dyn Signer,
    basket: &Basket,
    compute_unit_price: u64,
    blockhash: Hash,
) -> Result<VersionedTransaction> {
    let message = message(payer, basket, compute_unit_price, blockhash)?;
    let size = wire_size(&message);
    ensure!(
        message.account_keys.len() <= usize::from(MAX_ADDRESSES) && size <= MAX_TRANSACTION_SIZE,
        "{} accounts and {size} bytes exceed the v1 limits of {MAX_ADDRESSES} accounts and \
         {MAX_TRANSACTION_SIZE} bytes; configure fewer pools",
        message.account_keys.len()
    );
    Ok(VersionedTransaction::try_new(
        VersionedMessage::V1(message),
        &[payer],
    )?)
}

/// A single-signer v1 transaction's size: the version prefix, the message,
/// then the signature.
fn wire_size(message: &v1::Message) -> usize {
    1 + message.size() + SIGNATURE_SIZE
}

/// The executor instruction under a CU limit covering whichever route the
/// basket allows.
fn message(
    payer: &dyn Signer,
    basket: &Basket,
    compute_unit_price: u64,
    blockhash: Hash,
) -> Result<v1::Message> {
    // A random margin makes each transaction unique, so resends under one
    // blockhash are not deduplicated.
    let limit = basket
        .executor_cu()?
        .saturating_add(EXECUTOR_CU_MARGIN + rand::random::<u32>() % 1000)
        .min(MAX_COMPUTE_UNIT_LIMIT);
    Ok(v1::Message::try_compile_with_config(
        &payer.pubkey(),
        &[basket.instruction(limit - EXECUTOR_CU_MARGIN)?],
        blockhash,
        transaction_config(limit, compute_unit_price)?,
    )?)
}

/// A v1 transaction's resource limits, with its price in microlamports per CU.
pub fn transaction_config(
    compute_unit_limit: u32,
    compute_unit_price: u64,
) -> Result<v1::TransactionConfig> {
    let priority_fee =
        (u128::from(compute_unit_price) * u128::from(compute_unit_limit)).div_ceil(1_000_000);
    Ok(v1::TransactionConfig::empty()
        .with_compute_unit_limit(compute_unit_limit)
        .with_priority_fee(priority_fee.try_into()?)
        .with_loaded_accounts_data_size_limit(LOADED_ACCOUNTS_DATA_SIZE_LIMIT))
}

/// Sends `tx` through every client, returning how many accepted it.
pub fn send_transaction(
    clients: &[RpcClient],
    tx: &VersionedTransaction,
    max_retries: usize,
) -> usize {
    let config = RpcSendTransactionConfig {
        skip_preflight: true,
        max_retries: Some(max_retries),
        preflight_commitment: Some(CommitmentLevel::Confirmed),
        ..Default::default()
    };
    let mut accepted = 0;
    for (i, client) in clients.iter().enumerate() {
        match client.send_transaction_with_config(tx, config) {
            Ok(signature) => {
                info!("Transaction sent through RPC client {i}: {signature}");
                accepted += 1;
            }
            Err(e) => error!("Failed to send transaction through RPC client {i}: {e}"),
        }
    }
    accepted
}
