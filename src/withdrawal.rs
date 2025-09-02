// Withdrawal-related commands and logic for Clementine CLI

use crate::BitcoinAddress;
use crate::api_utils::get_tx_details;
use crate::bitcoin_utils::{
    DEFAULT_BRIDGE_CONTRACT_ADDRESS, DEPOSIT_AMOUNT_IN_HEX, sign_withdrawal_signature,
    verify_withdrawal_signature,
};
use crate::config::BridgeCliConfig;
use crate::errors::BridgeCliError;
use crate::structs::TaprootAddressWithPrefix;
use crate::types::{BRIDGE_CONTRACT, encode_safe_withdraw_params};
use crate::wallet::Purpose;
use crate::wallet::wallet_utils::{address_exists, validate_address_purpose};
use alloy::network::EthereumWallet;
use alloy::primitives::U256;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::TransactionReceipt;
use alloy::signers::Signer;
use alloy::signers::local::PrivateKeySigner;
use bitcoin::taproot::Signature;
use bitcoin::{Amount, Network, OutPoint, TxOut};
use bitcoincore_rpc::json::ScanTxOutRequest;
use bitcoincore_rpc::{Client, RpcApi};
use eyre::Context;
use open;
use serde_json::{Value, json};
use urlencoding::encode;

/// Parameters for safe withdrawal operations  
#[derive(Debug)]
pub struct SafeWithdrawalParams {
    pub signer_address: TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    pub withdrawal_address: BitcoinAddress,
    pub withdrawal_outpoint: OutPoint,
    pub withdrawal_amount: Amount,
    pub signature: bitcoin::taproot::Signature,
}

/// Helper function to securely load environment variable with better error handling
fn get_secret_key_from_env() -> Result<PrivateKeySigner, BridgeCliError> {
    let secret_key = std::env::var("SECRET_KEY")
        .map_err(|_| BridgeCliError::Eyre(eyre::eyre!("SECRET_KEY environment variable not found. Please set SECRET_KEY to proceed with this operation")))?;

    secret_key
        .parse::<PrivateKeySigner>()
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Invalid SECRET_KEY format: {}", e)))
}

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

    let keypair = crate::wallet::wallet_utils::load_key_with_purpose_check(
        signer_address,
        Purpose::Withdrawal,
    )?;

    let signature = sign_withdrawal_signature(
        &keypair,
        &signer_address.address,
        withdrawal_utxo,
        claim_address,
        *amount,
    )?;

    Ok(signature)
}

pub async fn safe_withdraw(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    withdrawal_address: &BitcoinAddress,
    withdrawal_outpoint: &OutPoint,
    withdrawal_amount: &Amount,
    sig: &bitcoin::taproot::Signature,
    config: &BridgeCliConfig,
) -> Result<String, BridgeCliError> {
    validate_address_purpose(signer_address, Purpose::Withdrawal)?;

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

    let params =
        prepare_withdrawal_params(withdrawal_outpoint, &payout_output, sig, config).await?;

    let calldata_hex =
        encode_safe_withdraw_params(&params.0, &params.1, &params.2, params.3, params.4);

    let tx_json = json!({
        "to": config.bridge_contract_address,
        "data": hex::encode(calldata_hex),
        "value": DEPOSIT_AMOUNT_IN_HEX,
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

pub async fn send_safe_withdrawal(
    params: SafeWithdrawalParams,
    config: &BridgeCliConfig,
) -> Result<TransactionReceipt, BridgeCliError> {
    // get the secret key from env with improved error handling
    let signer = get_secret_key_from_env()?;
    let chain_id: u64 = config.citrea_chain_id;
    let key = signer.with_chain_id(Some(chain_id));
    let wallet_address = key.address();

    tracing::debug!("Wallet address: {}", wallet_address);

    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(key))
        .connect_http(config.citrea_rpc_url.clone());

    validate_address_purpose(&params.signer_address, Purpose::Withdrawal)?;

    let payout_output = TxOut {
        value: params.withdrawal_amount,
        script_pubkey: params.withdrawal_address.script_pubkey(),
    };

    // verify signature
    verify_withdrawal_signature(
        &params.signature,
        &params.signer_address.address,
        &params.withdrawal_outpoint,
        &params.withdrawal_address,
        params.withdrawal_amount,
    )?;

    let withdrawal_params = prepare_withdrawal_params(
        &params.withdrawal_outpoint,
        &payout_output,
        &params.signature,
        config,
    )
    .await?;

    let bridge_contract_address = DEFAULT_BRIDGE_CONTRACT_ADDRESS;
    let contract = BRIDGE_CONTRACT::new(
        bridge_contract_address
            .parse()
            .expect("Correct contract address"),
        provider,
    );
    let citrea_withdrawal_tx = contract
        .safeWithdraw(
            withdrawal_params.0,
            withdrawal_params.1,
            withdrawal_params.2,
            withdrawal_params.3,
            withdrawal_params.4,
        )
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
    validate_address_purpose(signer_address, Purpose::Withdrawal)?;
    Ok(())
}

pub async fn scan_withdrawal(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    _claim_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<Vec<(OutPoint, Amount)>, BridgeCliError> {
    validate_address_purpose(signer_address, Purpose::Withdrawal)?;

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
    match get_utxos_from_mempool(address, config).await {
        Ok(utxos) => Ok(utxos),
        Err(mempool_error) => {
            tracing::warn!(
                "Mempool API failed for get_utxos_for_address: {}, falling back to Bitcoin RPC",
                mempool_error
            );

            // Fallback to Bitcoin RPC if available
            if config.bitcoin_config.is_some() {
                let rpc = config.connect_to_bitcoin_rpc().await?;
                get_utxos_from_rpc_with_client(address, &rpc).await
            } else {
                // If no Bitcoin RPC config, return the original mempool error
                Err(mempool_error)
            }
        }
    }
}

/// This might take a little while
async fn get_utxos_from_rpc_with_client(
    address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    rpc: &Client,
) -> Result<Vec<UtxoInfo>, BridgeCliError> {
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

/// Common withdrawal parameter preparation pattern (reduces major duplication)  
pub async fn prepare_withdrawal_params(
    withdrawal_outpoint: &OutPoint,
    payout_output: &TxOut,
    sig: &bitcoin::taproot::Signature,
    config: &BridgeCliConfig,
) -> Result<
    (
        crate::types::Transaction,
        crate::types::MerkleProof,
        crate::types::Transaction,
        alloy::sol_types::private::Bytes,
        alloy::sol_types::private::Bytes,
    ),
    BridgeCliError,
> {
    // Get the prepare tx details
    let (prepare_tx, prepare_tx_block, prepare_tx_block_height) =
        get_tx_details(&withdrawal_outpoint.txid, config).await?;

    let params = crate::parameters::get_citrea_safe_withdraw_params(
        withdrawal_outpoint,
        payout_output,
        sig,
        &prepare_tx,
        &prepare_tx_block,
        prepare_tx_block_height,
    )?;

    Ok(crate::types::prepare_safe_withdraw_params(
        &params.prepare_tx,
        &params.prepare_proof,
        &params.payout_tx,
        &params.block_header,
        &params.output_script_pk,
    ))
}
