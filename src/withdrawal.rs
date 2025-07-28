// Withdrawal-related commands and logic for Clementine CLI

use crate::bitcoin_utils::{
    confirm_private_key_storage, generate_key_and_taproot_address, sign_withdrawal_signature,
    verify_withdrawal_signature,
};
use crate::config::get_mempool_api_url;
use crate::deposit::parse_taproot_address;
use crate::parameters::get_citrea_safe_withdraw_params;
use crate::storage::{load_key, store_key};
use bitcoin::{Amount, Block, Network, OutPoint, Transaction, TxOut, Txid};
use colored::*;
use serde_json::Value;
use std::str::FromStr;

/// Generate a new signer key and taproot address for withdrawal operations
pub fn generate_signer_address(
    auto_yes: bool,
    network: Network,
) -> Result<(), Box<dyn std::error::Error>> {
    // Confirm with user about private key storage
    if !confirm_private_key_storage(auto_yes)? {
        println!("Operation cancelled by user.");
        return Ok(());
    }

    // Generate the key and address
    let (keypair, address) = generate_key_and_taproot_address(network)?;

    // Store the key securely
    let stored_address = store_key(&keypair, network, None)?;

    // Verify the stored address matches the generated one
    if stored_address != address {
        return Err("Address mismatch after storage".into());
    }

    // println!(
    //     "{} Signer key generated and stored successfully",
    //     "SUCCESS".green().bold()
    // );
    println!("{} {}", "ADDRESS".cyan().bold(), address);
    println!("{} {}", "NETWORK".blue().bold(), network);
    // println!("{} ~/.clementine/keys/", "STORAGE".magenta().bold());
    println!(
        "{} Please send 0.0000033 BTC (330 sats) to this address.",
        "INFO".yellow().bold()
    );

    Ok(())
}

pub fn generate_withdrawal_signature(
    signer_address: &str,
    claim_address: &str,
    withdrawal_utxo: &str,
    amount: f64,
    network: Network,
) -> Result<(), Box<dyn std::error::Error>> {
    let keypair = load_key(signer_address, network, None)?;

    let signer_address = parse_taproot_address(signer_address, network)?;
    let claim_address = parse_taproot_address(claim_address, network)?;
    let withdrawal_utxo = OutPoint::from_str(withdrawal_utxo)?;
    let amount = Amount::from_btc(amount)?;

    let signature = sign_withdrawal_signature(
        &keypair,
        &signer_address,
        &withdrawal_utxo,
        &claim_address,
        amount,
    )?;

    println!(
        "{} {}",
        "SIGNATURE".cyan().bold(),
        hex::encode(signature.serialize())
    );

    Ok(())
}

pub async fn get_tx_details_from_mempool(
    prepare_txid: &Txid,
    network: Network,
) -> Result<(Transaction, Block, u32), Box<dyn std::error::Error>> {
    let mempool_api_url = get_mempool_api_url(network);
    let url = format!("{mempool_api_url}tx/{prepare_txid}/hex");
    let response = reqwest::get(url).await.map_err(|e| format!("Failed to fetch transaction hex: {}", e))?;
    let tx_hex = response.text().await.map_err(|e| format!("Failed to read transaction hex response: {}", e))?;
    let tx: Transaction = bitcoin::consensus::deserialize(&hex::decode(tx_hex)?)?;
    debug!("tx: {:?}", tx);

    let url = format!("{mempool_api_url}tx/{prepare_txid}");
    let response = reqwest::get(url).await.map_err(|e| format!("Failed to fetch transaction data: {}", e))?;
    let tx_data: Value = response.json().await.map_err(|e| format!("Failed to parse transaction data: {}", e))?;
    debug!("tx_data: {:?}", tx_data);
    let block_hash = tx_data["status"]["block_hash"]
        .as_str()
        .ok_or("Block hash not found")?;
    let block_height = tx_data["status"]["block_height"]
        .as_u64()
        .ok_or("Block height not found")?;
    debug!("block_hash: {:?}", block_hash);
    debug!("block_height: {:?}", block_height);

    let url = format!("{mempool_api_url}block/{block_hash}/raw");
    let response = reqwest::get(url).await.unwrap();
    let block_raw = response.bytes().await.unwrap();
    debug!("block_raw: {:?}", block_raw);
    let block: Block = bitcoin::consensus::deserialize(&block_raw)?;
    debug!("block: {:?}", block);
    Ok((tx, block, block_height as u32))
}

pub fn get_tx_details_from_rpc(
    _bitcoind_rpc_url: &str,
    _bitcoind_rpc_user: &str,
    _bitcoind_rpc_password: &str,
    _prepare_txid: &Txid,
) -> Result<(Transaction, Block, u32), Box<dyn std::error::Error>> {
    unimplemented!();
}

pub async fn get_tx_details(
    prepare_txid: &Txid,
    bitcoind_rpc_url: Option<&str>,
    bitcoind_rpc_user: Option<&str>,
    bitcoind_rpc_password: Option<&str>,
    network: Network,
) -> Result<(Transaction, Block, u32), Box<dyn std::error::Error>> {
    if bitcoind_rpc_url.is_some() && bitcoind_rpc_user.is_some() && bitcoind_rpc_password.is_some()
    {
        let bitcoind_rpc_url = bitcoind_rpc_url.unwrap();
        let bitcoind_rpc_user = bitcoind_rpc_user.unwrap();
        let bitcoind_rpc_password = bitcoind_rpc_password.unwrap();

        let tx_details = get_tx_details_from_rpc(
            bitcoind_rpc_url,
            bitcoind_rpc_user,
            bitcoind_rpc_password,
            prepare_txid,
        );
        if tx_details.is_ok() {
            let (tx, block, block_height) = tx_details.unwrap();
            return Ok((tx, block, block_height));
        } else {
            println!("{}", "ERROR".red().bold());
            println!("Failed to get tx details from RPC");
            println!("Continuing with mempool.space");
        }
    }
    let tx_details = get_tx_details_from_mempool(prepare_txid, network).await;
    if tx_details.is_ok() {
        let (tx, block, block_height) = tx_details.unwrap();
        Ok((tx, block, block_height))
    } else {
        println!("{}", "ERROR".red().bold());
        println!("Failed to get tx details from mempool");
        Err("Failed to get tx details from mempool".into())
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn safe_withdraw(
    signer_address: &str,
    withdrawal_address: &str,
    withdrawal_utxo: &str,
    amount: f64,
    signature: &str,
    bitcoind_rpc_url: Option<&str>,
    bitcoind_rpc_user: Option<&str>,
    bitcoind_rpc_password: Option<&str>,
    network: Network,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Get the block and tx details for withdrawal
    let withdrawal_outpoint = OutPoint::from_str(withdrawal_utxo)?;
    let withdrawal_amount = Amount::from_btc(amount)?;
    // let input_amount = Amount::from_sat(330); // 0.0000033 BTC
    let sig = bitcoin::taproot::Signature::from_slice(&hex::decode(signature)?)?;
    let signer_address = parse_taproot_address(signer_address, network)?;
    let withdrawal_address = parse_taproot_address(withdrawal_address, network)?;

    let payout_output = TxOut {
        value: withdrawal_amount,
        script_pubkey: withdrawal_address.script_pubkey(),
    };

    // verify signature
    verify_withdrawal_signature(
        &sig,
        &signer_address,
        &withdrawal_outpoint,
        &withdrawal_address,
        withdrawal_amount,
    )?;

    // 2. Get the prepare tx details
    let (prepare_tx, prepare_tx_block, prepare_tx_block_height) = get_tx_details(
        &withdrawal_outpoint.txid,
        bitcoind_rpc_url,
        bitcoind_rpc_user,
        bitcoind_rpc_password,
        network,
    )
    .await?;

    get_citrea_safe_withdraw_params(
        &withdrawal_outpoint,
        &payout_output,
        &sig,
        &prepare_tx,
        &prepare_tx_block,
        prepare_tx_block_height,
    )?;

    // Prompt user to open the withdrawal UI
    let withdrawal_ui_url = "https://i-explorer.devnet.citrea.xyz/address/0x3100000000000000000000000000000000000002?tab=write_proxy#9072f747";
    println!("\n{} Press Enter to open the withdrawal UI in your default browser...", "INFO".yellow().bold());
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    if let Err(e) = open::that(withdrawal_ui_url) {
        println!("{} Failed to open browser: {}", "WARNING".yellow().bold(), e);
        println!("Please visit the following URL manually:\n{}", withdrawal_ui_url);
    }

    Ok(())
}

// TODO: Implement withdrawal.status
// TODO: Implement withdrawal.generate_operator_withdrawal_signatures
// TODO: Implement withdrawal.send_withdrawal_signatures_to_operators
