// Withdrawal-related commands and logic for Clementine CLI

use crate::bitcoin_utils::{sign_withdrawal_signature, verify_withdrawal_signature};
use crate::config::BridgeCliConfig;
use crate::errors::BridgeCliError;
use crate::parameters::get_citrea_safe_withdraw_params;
use crate::structs::AddressExt;
use crate::types::{BRIDGE_CONTRACT, prepare_safe_withdraw_params};
use crate::wallet::address::parse_address;
use crate::wallet::passphrase::prompt_unlock_passphrase;
use crate::wallet::wallet_utils::{load_address, load_key_and_address};
use alloy::network::EthereumWallet;
use alloy::primitives::U256;
use alloy::providers::ProviderBuilder;
use alloy::signers::Signer;
use alloy::signers::local::PrivateKeySigner;
use bitcoin::{Amount, Block, Network, OutPoint, Transaction, TxOut, Txid};
use bitcoincore_rpc::{Client, RpcApi};
use colored::*;
use eyre::Context;
use reqwest::Url;
use serde_json::Value;
use std::str::FromStr;

pub fn generate_withdrawal_signature(
    wallet_name: &str,
    claim_address: &str,
    withdrawal_utxo: &str,
    amount: f64,
    network: Network,
) -> Result<(), BridgeCliError> {
    let secure_passphrase = prompt_unlock_passphrase()?;
    let (keypair, signer_address) = load_key_and_address(wallet_name, network, &secure_passphrase)?;

    if !signer_address.is_taproot() {
        return Err(BridgeCliError::NotTaprootAddress);
    }

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

async fn get_tx_details_from_mempool(
    prepare_txid: &Txid,
    config: &BridgeCliConfig,
) -> Result<(Transaction, Block, u32), BridgeCliError> {
    let url = format!("{}tx/{prepare_txid}/hex", config.mempool_api_url);
    let response = reqwest::get(url)
        .await
        .map_err(|e| eyre::eyre!("Failed to fetch transaction hex for {prepare_txid}: {e}"))?;
    let tx_hex = response.text().await.map_err(|e| {
        eyre::eyre!("Failed to read transaction hex response for {prepare_txid}: {e}")
    })?;
    let tx: Transaction = bitcoin::consensus::deserialize(&hex::decode(tx_hex)?)?;
    debug!("tx: {:?}", tx);

    let url = format!("{}tx/{prepare_txid}", config.mempool_api_url);
    let response = reqwest::get(url)
        .await
        .wrap_err("Failed to fetch transaction data: {}")?;
    let tx_data: Value = response
        .json()
        .await
        .wrap_err("Failed to parse transaction data: {}")?;
    debug!("tx_data: {:?}", tx_data);
    let block_hash = tx_data["status"]["block_hash"]
        .as_str()
        .ok_or(eyre::eyre!("Block hash not found"))?;
    let block_height = tx_data["status"]["block_height"]
        .as_u64()
        .ok_or(eyre::eyre!("Block height not found"))?;
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

async fn get_tx_details_from_rpc(
    rpc: &Client,
    prepare_txid: &Txid,
) -> Result<(Transaction, Block, u32), BridgeCliError> {
    let tx = rpc.get_raw_transaction(prepare_txid, None).await?;
    let tx_info = rpc.get_raw_transaction_info(prepare_txid, None).await?;
    if tx_info.blockhash.is_none() {
        return Err(eyre::eyre!("Block hash not found, maybe not confirmed yet").into());
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

pub(crate) async fn get_txout_details(
    config: &BridgeCliConfig,
    txid: &Txid,
    vout: u32,
) -> Result<TxOut, BridgeCliError> {
    let (tx, _, _) = get_tx_details(txid, config).await?;
    let txout = tx
        .output
        .get(vout as usize)
        .ok_or::<BridgeCliError>(eyre::eyre!("Txout not found").into())?;

    Ok(txout.clone())
}

pub(crate) async fn get_tx_details(
    prepare_txid: &Txid,
    config: &BridgeCliConfig,
) -> Result<(Transaction, Block, u32), BridgeCliError> {
    match config.bitcoin_config {
        Some(_) => {
            let rpc = config.connect_to_bitcoin_rpc().await?;
            Ok(get_tx_details_from_rpc(&rpc, prepare_txid).await?)
        }
        None => get_tx_details_from_mempool(prepare_txid, config).await,
    }
}

pub async fn safe_withdraw(
    wallet_name: &str,
    withdrawal_address: &str,
    withdrawal_utxo: &str,
    amount: f64,
    signature: &str,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    // 1. Get the block and tx details for withdrawal
    let withdrawal_outpoint = OutPoint::from_str(withdrawal_utxo)?;
    let withdrawal_amount = Amount::from_btc(amount)?;
    // let input_amount = Amount::from_sat(330); // 0.0000033 BTC
    let sig = bitcoin::taproot::Signature::from_slice(&hex::decode(signature)?)
        .wrap_err("Can't parse taproot signature")?;
    let signer_address = load_address(wallet_name, None, config.network)?;
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
    std::io::stdin()
        .read_line(&mut input)
        .wrap_err("Can't read key stroke")?;

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
    wallet_name: &str,
    withdrawal_address: &str,
    withdrawal_utxo: &str,
    amount: f64,
    signature: &str,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    // get the secret key from env
    // raise error if not found
    let secret_key = std::env::var("SECRET_KEY").map_err(|_| eyre::eyre!("SECRET_KEY not found, for this command, you need to set the SECRET_KEY environment variable"))?;
    let signer: PrivateKeySigner = secret_key
        .parse()
        .map_err(|e| eyre::eyre!("Failed to parse SECRET_KEY: {e}"))?;
    let chain_id: u64 = config.citrea_chain_id;
    let key = signer.with_chain_id(Some(chain_id));
    let wallet_address = key.address();

    debug!("Wallet address: {}", wallet_address);

    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(key))
        .connect_http(Url::parse(&config.citrea_rpc_url).wrap_err("Can't parse url")?);

    // 1. Get the block and tx details for withdrawal
    let withdrawal_outpoint = OutPoint::from_str(withdrawal_utxo)?;
    let withdrawal_amount = Amount::from_btc(amount)?;
    // let input_amount = Amount::from_sat(330); // 0.0000033 BTC
    let sig = bitcoin::taproot::Signature::from_slice(&hex::decode(signature)?)
        .wrap_err("Can't parse signature")?;
    let signer_address = load_address(wallet_name, None, config.network)?;
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

    let receipt = citrea_withdrawal_tx
        .get_receipt()
        .await
        .wrap_err("Can't get receipt")?;
    println!("Citrea withdrawal tx receipt: {:?}", receipt);

    Ok(())
}
