// Withdrawal-related commands and logic for Clementine CLI

use crate::bitcoin_utils::{
    confirm_private_key_storage, generate_key_and_taproot_address, sign_withdrawal_signature,
    verify_withdrawal_signature,
};
use crate::config::CliConfig;
use crate::deposit::{parse_address, parse_taproot_address};
use crate::parameters::get_citrea_safe_withdraw_params;
use crate::passphrase::{prompt_new_passphrase, prompt_unlock_passphrase};
use crate::types::{BRIDGE_CONTRACT, prepare_safe_withdraw_params};
use crate::wallet::{load_key, store_key};
use alloy::network::EthereumWallet;
use alloy::primitives::U256;
use alloy::providers::ProviderBuilder;
use alloy::signers::Signer;
use alloy::signers::local::PrivateKeySigner;
use anyhow::anyhow;
use bitcoin::{Amount, Block, Network, OutPoint, Transaction, TxOut, Txid};
use bitcoincore_rpc::{Client, RpcApi};
use colored::*;
use reqwest::Url;
use serde_json::Value;
use std::str::FromStr;

/// Generate a new signer key and taproot address for withdrawal operations
pub fn generate_signer_address(auto_yes: bool, network: Network) -> Result<(), anyhow::Error> {
    // Confirm with user about private key storage
    if !confirm_private_key_storage(auto_yes)? {
        println!("Operation cancelled by user.");
        return Ok(());
    }

    // Generate the key and address
    let (keypair, address) = generate_key_and_taproot_address(network, 1)?;

    // Prompt for passphrase to encrypt the key
    let secure_passphrase = prompt_new_passphrase()?;

    // Store the key securely
    let stored_address = store_key(&keypair, network, secure_passphrase)?;

    // Verify the stored address matches the generated one
    if stored_address != address {
        return Err(anyhow!("Address mismatch after storage"));
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
) -> Result<(), anyhow::Error> {
    // Try loading key without passphrase first, if that fails, prompt for passphrase
    let keypair = match load_key(signer_address, network, None) {
        Ok(keypair) => keypair,
        Err(_) => {
            // Key might be encrypted, prompt for passphrase
            println!("Key appears to be encrypted. Please enter the passphrase:");
            let secure_passphrase = prompt_unlock_passphrase()?;
            load_key(signer_address, network, Some(&secure_passphrase))?
        }
    };

    let signer_address = parse_taproot_address(signer_address, network)?;
    let claim_address = parse_address(claim_address, network)?;
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
    config: &CliConfig,
) -> Result<(Transaction, Block, u32), anyhow::Error> {
    let url = format!("{}tx/{prepare_txid}/hex", config.mempool_api_url);
    let response = reqwest::get(url)
        .await
        .map_err(|e| anyhow!("Failed to fetch transaction hex: {e}"))?;
    let tx_hex = response
        .text()
        .await
        .map_err(|e| anyhow!("Failed to read transaction hex response: {e}"))?;
    let tx: Transaction = bitcoin::consensus::deserialize(&hex::decode(tx_hex)?)?;
    debug!("tx: {:?}", tx);

    let url = format!("{}tx/{prepare_txid}", config.mempool_api_url);
    let response = reqwest::get(url)
        .await
        .map_err(|e| anyhow!("Failed to fetch transaction data: {e}"))?;
    let tx_data: Value = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse transaction data: {e}"))?;
    debug!("tx_data: {:?}", tx_data);
    let block_hash = tx_data["status"]["block_hash"]
        .as_str()
        .ok_or(anyhow!("Block hash not found"))?;
    let block_height = tx_data["status"]["block_height"]
        .as_u64()
        .ok_or(anyhow!("Block height not found"))?;
    debug!("block_hash: {:?}", block_hash);
    debug!("block_height: {:?}", block_height);

    let url = format!("{}block/{block_hash}/raw", config.mempool_api_url);
    let response = reqwest::get(url).await.unwrap();
    let block_raw = response.bytes().await.unwrap();
    debug!("block_raw: {:?}", block_raw);
    let block: Block = bitcoin::consensus::deserialize(&block_raw)?;
    debug!("block: {:?}", block);
    Ok((tx, block, block_height as u32))
}

pub async fn get_tx_details_from_rpc(
    rpc: &Client,
    prepare_txid: &Txid,
) -> Result<(Transaction, Block, u32), anyhow::Error> {
    let tx = rpc.get_raw_transaction(prepare_txid, None).await?;
    let tx_info = rpc.get_raw_transaction_info(prepare_txid, None).await?;
    if tx_info.blockhash.is_none() {
        return Err(anyhow!("Block hash not found, maybe not confirmed yet"));
    }
    let block = rpc.get_block(&tx_info.blockhash.unwrap()).await?;
    let block_height = rpc
        .get_block_header_info(&tx_info.blockhash.unwrap())
        .await?
        .height;
    debug!("tx_info: {:?}", tx_info);
    debug!("block: {:?}", block);
    debug!("block_height: {:?}", block_height);

    Ok((tx, block, block_height as u32))
}

pub async fn get_txout_details(
    config: &CliConfig,
    txid: &Txid,
    vout: u32,
) -> Result<TxOut, anyhow::Error> {
    let (tx, _, _) = get_tx_details(txid, config).await?;
    let txout = tx
        .output
        .get(vout as usize)
        .ok_or(anyhow!("Txout not found"))?;
    Ok(txout.clone())
}

pub async fn get_tx_details(
    prepare_txid: &Txid,
    config: &CliConfig,
) -> Result<(Transaction, Block, u32), anyhow::Error> {
    match config.bitcoin_config {
        Some(_) => {
            let rpc = config.connect_to_bitcoin_rpc().await?;
            Ok(get_tx_details_from_rpc(&rpc, prepare_txid).await?)
        }
        None => get_tx_details_from_mempool(prepare_txid, config).await,
    }
}

pub async fn safe_withdraw(
    signer_address: &str,
    withdrawal_address: &str,
    withdrawal_utxo: &str,
    amount: f64,
    signature: &str,
    config: &CliConfig,
) -> Result<(), anyhow::Error> {
    // 1. Get the block and tx details for withdrawal
    let withdrawal_outpoint = OutPoint::from_str(withdrawal_utxo)?;
    let withdrawal_amount = Amount::from_btc(amount)?;
    // let input_amount = Amount::from_sat(330); // 0.0000033 BTC
    let sig = bitcoin::taproot::Signature::from_slice(&hex::decode(signature)?)?;
    let signer_address = parse_taproot_address(signer_address, config.network)?;
    let withdrawal_address = parse_address(withdrawal_address, config.network)?;

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
    let (prepare_tx, prepare_tx_block, prepare_tx_block_height) =
        get_tx_details(&withdrawal_outpoint.txid, config).await?;

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
    println!(
        "\n{} Press Enter to open the withdrawal UI in your default browser...",
        "INFO".yellow().bold()
    );
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    if let Err(e) = open::that(withdrawal_ui_url) {
        println!(
            "{} Failed to open browser: {}",
            "WARNING".yellow().bold(),
            e
        );
        println!("Please visit the following URL manually:\n{withdrawal_ui_url}",);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn send_safe_withdrawal(
    signer_address: &str,
    withdrawal_address: &str,
    withdrawal_utxo: &str,
    amount: f64,
    signature: &str,
    config: &CliConfig,
) -> Result<(), anyhow::Error> {
    // get the secret key from env
    // raise error if not found
    let secret_key = std::env::var("SECRET_KEY").map_err(|_| anyhow!("SECRET_KEY not found, for this command, you need to set the SECRET_KEY environment variable"))?;
    let signer: PrivateKeySigner = secret_key.parse()?;
    let chain_id: u64 = config.citrea_chain_id;
    let key = signer.with_chain_id(Some(chain_id));
    let wallet_address = key.address();

    debug!("Wallet address: {}", wallet_address);

    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(key))
        .connect_http(Url::parse(&config.citrea_rpc_url)?);

    // 1. Get the block and tx details for withdrawal
    let withdrawal_outpoint = OutPoint::from_str(withdrawal_utxo)?;
    let withdrawal_amount = Amount::from_btc(amount)?;
    // let input_amount = Amount::from_sat(330); // 0.0000033 BTC
    let sig = bitcoin::taproot::Signature::from_slice(&hex::decode(signature)?)?;
    let signer_address = parse_taproot_address(signer_address, config.network)?;
    let withdrawal_address = parse_address(withdrawal_address, config.network)?;

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
    let (prepare_tx, prepare_tx_block, prepare_tx_block_height) =
        get_tx_details(&withdrawal_outpoint.txid, config).await?;

    let params = get_citrea_safe_withdraw_params(
        &withdrawal_outpoint,
        &payout_output,
        &sig,
        &prepare_tx,
        &prepare_tx_block,
        prepare_tx_block_height,
    )?;

    let (prepare_tx, prepare_proof, payout_tx_params, block_header, output_script_pk) = params;
    let params = prepare_safe_withdraw_params(
        &prepare_tx,
        &prepare_proof,
        &payout_tx_params,
        &block_header,
        &output_script_pk,
    );

    let bridge_contract_address = "0x3100000000000000000000000000000000000002";
    let contract = BRIDGE_CONTRACT::new(
        bridge_contract_address
            .parse()
            .expect("Correct contract address"),
        provider,
    );
    const SATS_TO_WEI_MULTIPLIER: u64 = 10_000_000_000;

    let citrea_withdrawal_tx = contract
        .safeWithdraw(params.0, params.1, params.2, params.3, params.4)
        .value(U256::from(
            config.bridge_amount.to_sat() * SATS_TO_WEI_MULTIPLIER,
        ))
        .send()
        .await?;

    let receipt = citrea_withdrawal_tx.get_receipt().await?;
    println!("Citrea withdrawal tx receipt: {receipt:?}");

    Ok(())
}
