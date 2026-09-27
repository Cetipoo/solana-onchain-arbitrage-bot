use crate::transaction::transaction_config;
use crate::v10::{SOL, TOKEN, USDC};
use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::Instruction,
    message::{v1, VersionedMessage},
    signature::Keypair,
    signer::Signer,
    transaction::VersionedTransaction,
};
use spl_associated_token_account_interface::{
    address::get_associated_token_address, instruction::create_associated_token_account_idempotent,
};
use tracing::info;

/// Creates whichever of the wallet's WSOL and USDC token accounts, which
/// every transaction settles through, do not exist yet.
pub fn create_missing_settlement_atas(rpc: &RpcClient, wallet: &Keypair) -> Result<()> {
    let owner = wallet.pubkey();
    let mints = [SOL, USDC];
    let atas = mints.map(|mint| get_associated_token_address(&owner, &mint));
    info!("Settlement ATAs: WSOL {}, USDC {}", atas[0], atas[1]);
    let missing: Vec<Instruction> = mints
        .iter()
        .zip(rpc.get_multiple_accounts(&atas)?)
        .filter(|(_, account)| account.is_none())
        .map(|(mint, _)| create_associated_token_account_idempotent(&owner, &owner, mint, &TOKEN))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let message = v1::Message::try_compile_with_config(
        &owner,
        &missing,
        rpc.get_latest_blockhash()?,
        transaction_config(60_000 * missing.len() as u32, 1_000_000)?,
    )?;
    let tx = VersionedTransaction::try_new(VersionedMessage::V1(message), &[wallet])?;
    let signature = rpc
        .send_and_confirm_transaction(&tx)
        .context("Failed to create settlement ATAs")?;
    info!("Created {} settlement ATA(s): {}", missing.len(), signature);
    Ok(())
}
