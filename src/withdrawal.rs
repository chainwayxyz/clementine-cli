// Withdrawal-related commands and logic for Clementine CLI

use crate::BitcoinAddress;
use crate::bitcoin_utils::{sign_withdrawal_signature, verify_withdrawal_signature};
use crate::config::BridgeCliConfig;
use crate::errors::BridgeCliError;
use crate::structs::TaprootAddressWithPrefix;
use crate::types::{BRIDGE_CONTRACT, encode_safe_withdraw_params};
use crate::wallet::Purpose;
use crate::wallet::wallet_utils::address_exists;
use alloy::network::EthereumWallet;
use alloy::primitives::U256;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::TransactionReceipt;
use alloy::signers::Signer;
use alloy::signers::local::PrivateKeySigner;
use bitcoin::taproot::Signature;
use bitcoin::{Amount, Block, Network, OutPoint, Transaction, TxOut, Txid};
use bitcoincore_rpc::json::ScanTxOutRequest;
use bitcoincore_rpc::{Client, RpcApi};
use eyre::Context;
use open;
use serde_json::{Value, json};
use urlencoding::encode;

pub fn generate_withdrawal_signature(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    claim_address: &BitcoinAddress,
    withdrawal_utxo: &OutPoint,
    amount: &Amount,
    network: Network,
) -> Result<Signature, BridgeCliError> {
    let claim_wallet_address = TaprootAddressWithPrefix::from_string_without_prefix(
        &claim_address.to_string(),
        Purpose::Withdrawal,
        network,
    )?;

    // Check if the claim address belongs to any of our wallets
    if address_exists(&claim_wallet_address)? {
        return Err(BridgeCliError::ClaimAddressIsWalletAddress);
    }

    let keypair =
        crate::bitcoin_utils::load_key_with_purpose_check(signer_address, Purpose::Withdrawal)?;

    let signature = sign_withdrawal_signature(
        &keypair,
        &signer_address.address,
        withdrawal_utxo,
        claim_address,
        *amount,
    )?;

    Ok(signature)
}

async fn get_tx_details_from_mempool(
    prepare_txid: &Txid,
    config: &BridgeCliConfig,
) -> Result<(Transaction, Block, u32), BridgeCliError> {
    let url = config
        .mempool_api_url
        .join(&format!("tx/{prepare_txid}/hex"))
        .wrap_err("Can't join url in get_tx_details_from_mempool")?;
    let response = reqwest::get(url)
        .await
        .map_err(|e| eyre::eyre!("Failed to fetch transaction hex for {prepare_txid}: {e}"))?;
    let tx_hex = response.text().await.map_err(|e| {
        eyre::eyre!("Failed to read transaction hex response for {prepare_txid}: {e}")
    })?;
    let tx: Transaction = bitcoin::consensus::deserialize(&hex::decode(tx_hex)?)?;
    tracing::debug!("tx: {:?}", tx);

    let url = config
        .mempool_api_url
        .join(&format!("tx/{prepare_txid}"))
        .wrap_err("Can't join url in get_tx_details_from_mempool")?;
    let response = reqwest::get(url)
        .await
        .wrap_err("Failed to fetch transaction data: {}")?;
    let tx_data: Value = response
        .json()
        .await
        .wrap_err("Failed to parse transaction data: {}")?;
    tracing::debug!("tx_data: {:?}", tx_data);
    let block_hash = tx_data["status"]["block_hash"]
        .as_str()
        .ok_or(eyre::eyre!("Block hash not found"))?;
    let block_height = tx_data["status"]["block_height"]
        .as_u64()
        .ok_or(eyre::eyre!("Block height not found"))?;
    tracing::debug!("block_hash: {:?}", block_hash);
    tracing::debug!("block_height: {:?}", block_height);

    let url = config
        .mempool_api_url
        .join(&format!("block/{block_hash}/raw"))
        .wrap_err("Can't join url in get_tx_details_from_mempool")?;
    let response = reqwest::get(url).await.unwrap();
    let block_raw = response.bytes().await.unwrap();
    tracing::debug!("block_raw: {:?}", block_raw);
    let block: Block = bitcoin::consensus::deserialize(&block_raw)?;
    tracing::debug!("block: {:?}", block);
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
    tracing::debug!("tx_info: {:?}", tx_info);
    tracing::debug!("block: {:?}", block);
    tracing::debug!("block_height: {:?}", block_height);

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
    match get_tx_details_from_mempool(prepare_txid, config).await {
        Ok(result) => Ok(result),
        Err(mempool_error) => {
            tracing::warn!(
                "Mempool API failed: {}, falling back to Bitcoin RPC",
                mempool_error
            );

            if config.bitcoin_config.is_some() {
                let rpc = config.connect_to_bitcoin_rpc().await?;
                get_tx_details_from_rpc(&rpc, prepare_txid).await
            } else {
                Err(mempool_error)
            }
        }
    }
}

pub async fn safe_withdraw(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    withdrawal_address: &BitcoinAddress,
    withdrawal_outpoint: &OutPoint,
    withdrawal_amount: &Amount,
    sig: &bitcoin::taproot::Signature,
    config: &BridgeCliConfig,
) -> Result<String, BridgeCliError> {
    crate::bitcoin_utils::validate_address_purpose(signer_address, Purpose::Withdrawal)?;

    let payout_output = TxOut {
        value: *withdrawal_amount,
        script_pubkey: withdrawal_address.script_pubkey(),
    };

    // verify signature
    verify_withdrawal_signature(
        sig,
        &signer_address.address,
        withdrawal_outpoint,
        withdrawal_address,
        *withdrawal_amount,
    )?;

    let params = crate::bitcoin_utils::prepare_withdrawal_params(
        withdrawal_outpoint,
        &payout_output,
        sig,
        config,
    )
    .await?;

    let calldata_hex =
        encode_safe_withdraw_params(&params.0, &params.1, &params.2, params.3, params.4);

    let tx_json = json!({
        "to": config.bridge_contract_address,
        "data": hex::encode(calldata_hex),
        "value": "0x8AC7230489E80000",
        "chainId": config.citrea_chain_id,
    })
    .to_string();

    // Prompt user to open the withdrawal UI
    let query = format!(
        "?transaction_request={}&withdrawal_address={}",
        encode(&tx_json),
        encode(&withdrawal_address.to_string())
    );
    let withdrawal_ui_url = format!("{}{}", config.get_withdrawal_sign_url(), query);

    if let Err(e) = open::that(&withdrawal_ui_url) {
        return Err(eyre::eyre!(
            "Failed to open browser: {}. Please visit the following URL manually: {}",
            e,
            withdrawal_ui_url
        )
        .into());
    }

    Ok(withdrawal_ui_url)
}

#[allow(clippy::too_many_arguments)]
pub async fn send_safe_withdrawal(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    withdrawal_address: &BitcoinAddress,
    withdrawal_outpoint: &OutPoint,
    withdrawal_amount: &Amount,
    sig: &bitcoin::taproot::Signature,
    config: &BridgeCliConfig,
) -> Result<TransactionReceipt, BridgeCliError> {
    // get the secret key from env
    // raise error if not found
    let secret_key = std::env::var("SECRET_KEY").map_err(|e| eyre::eyre!("SECRET_KEY not found, for this command, you need to set the SECRET_KEY environment variable: {e}"))?;
    let signer: PrivateKeySigner = secret_key
        .parse()
        .map_err(|e| eyre::eyre!("Failed to parse SECRET_KEY: {e}"))?;
    let chain_id: u64 = config.citrea_chain_id;
    let key = signer.with_chain_id(Some(chain_id));
    let wallet_address = key.address();

    tracing::debug!("Wallet address: {}", wallet_address);

    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(key))
        .connect_http(config.citrea_rpc_url.clone());

    crate::bitcoin_utils::validate_address_purpose(signer_address, Purpose::Withdrawal)?;

    let payout_output = TxOut {
        value: *withdrawal_amount,
        script_pubkey: withdrawal_address.script_pubkey(),
    };

    // verify signature
    verify_withdrawal_signature(
        sig,
        &signer_address.address,
        withdrawal_outpoint,
        withdrawal_address,
        *withdrawal_amount,
    )?;

    let params = crate::bitcoin_utils::prepare_withdrawal_params(
        withdrawal_outpoint,
        &payout_output,
        sig,
        config,
    )
    .await?;

    let bridge_contract_address = "0x3100000000000000000000000000000000000002";
    let contract = BRIDGE_CONTRACT::new(
        bridge_contract_address
            .parse()
            .expect("Correct contract address"),
        provider,
    );
    let citrea_withdrawal_tx = contract
        .safeWithdraw(params.0, params.1, params.2, params.3, params.4)
        .value(U256::from(
            config.bridge_amount.to_sat() * crate::bitcoin_utils::SATS_TO_WEI_MULTIPLIER,
        ))
        .send()
        .await?;

    let receipt = citrea_withdrawal_tx
        .get_receipt()
        .await
        .wrap_err("Can't get receipt")?;

    Ok(receipt)
}

pub(crate) fn start_withdrawal(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    _claim_address: &BitcoinAddress,
    _config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    crate::bitcoin_utils::validate_address_purpose(signer_address, Purpose::Withdrawal)?;
    Ok(())
}

pub async fn scan_withdrawal(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    _claim_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<Vec<(OutPoint, Amount)>, BridgeCliError> {
    crate::bitcoin_utils::validate_address_purpose(signer_address, Purpose::Withdrawal)?;

    let utxos = get_utxos_for_address(signer_address, config).await?;
    let mut results = Vec::new();

    for utxo in utxos {
        let withdrawal_outpoint = OutPoint {
            txid: utxo.txid,
            vout: utxo.vout,
        };
        results.push((withdrawal_outpoint, utxo.value));
    }

    Ok(results)
}

#[derive(Debug)]
struct UtxoInfo {
    txid: bitcoin::Txid,
    vout: u32,
    value: Amount,
}

async fn get_utxos_for_address(
    address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    config: &BridgeCliConfig,
) -> Result<Vec<UtxoInfo>, BridgeCliError> {
    // Try mempool API first
    match get_utxos_from_mempool(address, config).await {
        Ok(utxos) => Ok(utxos),
        Err(mempool_error) => {
            tracing::warn!(
                "Mempool API failed: {}, falling back to Bitcoin RPC",
                mempool_error
            );

            // Fallback to Bitcoin RPC if available
            if config.bitcoin_config.is_some() {
                get_utxos_from_rpc(address, config).await
            } else {
                // If no Bitcoin RPC config, return the original mempool error
                Err(mempool_error)
            }
        }
    }
}

/// This might take a little while
async fn get_utxos_from_rpc(
    address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    config: &BridgeCliConfig,
) -> Result<Vec<UtxoInfo>, BridgeCliError> {
    let rpc = config.connect_to_bitcoin_rpc().await?;

    let res = rpc
        .scan_tx_out_set_blocking(&[ScanTxOutRequest::Single(format!(
            "addr({})",
            address.address
        ))])
        .await?;

    let mut result = Vec::new();
    for utxo in res.unspents {
        if utxo.amount == Amount::from_sat(crate::bitcoin_utils::WITHDRAWAL_UTXO_AMOUNT) {
            result.push(UtxoInfo {
                txid: utxo.txid,
                vout: utxo.vout,
                value: utxo.amount,
            });
        }
    }

    Ok(result)
}

async fn get_utxos_from_mempool(
    address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    config: &BridgeCliConfig,
) -> Result<Vec<UtxoInfo>, BridgeCliError> {
    use std::str::FromStr;

    let url = config
        .mempool_api_url
        .join(&format!("address/{}/utxo", address.address))
        .wrap_err("Can't join URL for address UTXOs")?;

    tracing::debug!("Fetching UTXOs from URL: {}", url);

    let response = reqwest::get(url)
        .await
        .map_err(|e| eyre::eyre!("Failed to fetch UTXOs for address {}: {e}", address.address))?;

    tracing::debug!("UTXO response: {}", response.status());

    let utxos: Value = response
        .json()
        .await
        .wrap_err("Failed to parse UTXO response")?;

    let mut result = Vec::new();

    if let Some(utxo_array) = utxos.as_array() {
        for utxo in utxo_array {
            if let (Some(txid_str), Some(vout), Some(value)) = (
                utxo["txid"].as_str(),
                utxo["vout"].as_u64(),
                utxo["value"].as_u64(),
            ) && value == crate::bitcoin_utils::WITHDRAWAL_UTXO_AMOUNT
            {
                let txid = bitcoin::Txid::from_str(txid_str)
                    .map_err(|e| eyre::eyre!("Invalid txid: {e}"))?;
                result.push(UtxoInfo {
                    txid,
                    vout: vout as u32,
                    value: Amount::from_sat(value),
                });
            }
        }
    }

    Ok(result)
}
