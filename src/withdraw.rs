// Withdrawal-related commands and logic for Clementine CLI
use crate::BitcoinAddress;
use crate::api_utils::{get_tx_details, get_utxos, is_tx_on_chain};
use crate::bitcoin_utils::{sign_withdrawal_signature, verify_withdrawal_signature};
use crate::config::BridgeCliConfig;
use crate::errors::BridgeCliError;
use crate::secure_types::SecureKeypair;
use crate::sqlite_db::sqlite_client::SqliteDb;
use crate::structs::{TaprootAddressWithPrefix, WithdrawalParams};
use crate::types::{BRIDGE_CONTRACT, CitreaContract, encode_safe_withdraw_params};
use crate::wallet::Purpose;
use crate::wallet::wallet_utils::is_wallet_address;
use crate::wallet::wallet_utils::{ensure_wallet_exists, validate_address_purpose};
use alloy::network::EthereumWallet;
use alloy::primitives::U256;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::TransactionReceipt;
use alloy::signers::Signer;
use alloy::signers::local::PrivateKeySigner;
use bitcoin::taproot::Signature;
use bitcoin::{Amount, OutPoint, TxOut};
use eyre::Context;
use secrecy::ExposeSecret;
use serde_json::json;
use urlencoding::encode;

pub(crate) enum WithdrawStatusEnum {
    New,
    InProgress,
    OptimisticPayoutFailed,
    Completed,
    Unknown,
}

#[derive(Debug)]
pub struct WithdrawalUrl(pub String);

#[derive(Debug)]
pub struct TxJson(pub String);

impl WithdrawStatusEnum {
    pub(crate) fn from_backend_status(status: &str) -> Self {
        match status {
            "new" => WithdrawStatusEnum::New,
            "completed" => WithdrawStatusEnum::Completed,
            "sending-to-optimistic-payout"
            | "sent-to-optimistic-payout"
            | "sending-to-operator-withdraw"
            | "sent-to-operator-withdraw" => WithdrawStatusEnum::InProgress,
            "optimistic-payout-failed" => WithdrawStatusEnum::OptimisticPayoutFailed,
            unknown_status => {
                tracing::debug!("Returned unknown status: {}", unknown_status);
                WithdrawStatusEnum::Unknown
            }
        }
    }
    pub fn as_string(&self) -> String {
        match self {
            WithdrawStatusEnum::New => "New".to_string(),
            WithdrawStatusEnum::InProgress => "In Progress".to_string(),
            WithdrawStatusEnum::OptimisticPayoutFailed => {
                "Optimistic payout failed! Please proceed with operator paid withdrawal..."
                    .to_string()
            }
            WithdrawStatusEnum::Completed => "Completed".to_string(),
            WithdrawStatusEnum::Unknown => "Unknown".to_string(),
        }
    }
}

/// Parameters for safe withdrawal operations  
#[derive(Debug)]
pub struct SafeWithdrawalParams {
    pub signer_address: TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    pub destination_address: BitcoinAddress,
    pub withdrawal_outpoint: OutPoint,
    pub withdrawal_amount: Amount,
    pub signature: bitcoin::taproot::Signature,
}

fn create_bridge_contract(
    key: PrivateKeySigner,
    config: &BridgeCliConfig,
) -> Result<CitreaContract, BridgeCliError> {
    let citrea_rpc_url =
        config
            .citrea_rpc_url
            .as_ref()
            .ok_or(BridgeCliError::Eyre(eyre::eyre!(
                "CITREA_RPC_URL is not set in the configuration. Please set it to proceed."
            )))?;

    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(key))
        .connect_http(citrea_rpc_url.clone());

    let contract = BRIDGE_CONTRACT::new(
        config
            .bridge_contract_address
            .parse()
            .wrap_err("Failed to parse bridge contract address")?,
        provider,
    );

    Ok(contract)
}

#[allow(clippy::too_many_arguments)]
pub async fn generate_withdrawal_signatures(
    keypair: SecureKeypair,
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    destination_address: &BitcoinAddress,
    withdrawal_utxo: &OutPoint,
    optimistic_withdrawal_amount: &Amount,
    operator_withdrawal_amount: &Amount,
    config: &BridgeCliConfig,
    sqlite_client: Option<&SqliteDb>,
) -> Result<(Signature, Signature), BridgeCliError> {
    ensure_wallet_exists(signer_address, sqlite_client).await?;

    if signer_address.purpose != Purpose::Withdrawal {
        return Err(BridgeCliError::PurposeMismatch {
            expected: Purpose::Withdrawal,
            found: signer_address.purpose,
        });
    }

    // If the claim address is a Taproot address, ensure it is not a Clementine wallet address
    if is_wallet_address(destination_address, config).await? {
        return Err(BridgeCliError::DestinationAddressIsWalletAddress);
    }

    let optimistic_withdrawal_signature = sign_withdrawal_signature(
        &keypair,
        &signer_address.address,
        withdrawal_utxo,
        destination_address,
        *optimistic_withdrawal_amount,
        config,
    )?;

    let operator_withdrawal_signature = sign_withdrawal_signature(
        &keypair,
        &signer_address.address,
        withdrawal_utxo,
        destination_address,
        *operator_withdrawal_amount,
        config,
    )?;

    Ok((
        optimistic_withdrawal_signature,
        operator_withdrawal_signature,
    ))
}

pub async fn safe_withdraw(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    destination_address: &BitcoinAddress,
    withdrawal_outpoint: &OutPoint,
    withdrawal_amount: &Amount,
    sig: &bitcoin::taproot::Signature,
    config: &BridgeCliConfig,
) -> Result<(WithdrawalUrl, TxJson, WithdrawalParams), BridgeCliError> {
    validate_address_purpose(signer_address, Purpose::Withdrawal)?;

    let payout_output = TxOut {
        value: *withdrawal_amount,
        script_pubkey: destination_address.script_pubkey(),
    };

    // verify signature
    verify_withdrawal_signature(
        sig,
        &signer_address.address,
        withdrawal_outpoint,
        destination_address,
        *withdrawal_amount,
        config,
    )?;

    let params =
        prepare_withdrawal_params(withdrawal_outpoint, &payout_output, sig, config).await?;

    let calldata_hex = encode_safe_withdraw_params(
        &params.transaction,
        &params.merkle_proof,
        &params.payout_transaction,
        &params.block_header,
        &params.output_script_pk,
    );

    let tx_json = json!({
        "to": config.bridge_contract_address,
        "data": hex::encode(calldata_hex),
        "value": format!("0x{:X}", config.bridge_amount.to_sat() * crate::bitcoin_utils::SATS_TO_WEI_MULTIPLIER),
        "chainId": config.citrea_chain_id,
    })
    .to_string();

    // Prompt user to open the withdrawal UI
    let query = format!(
        "?transaction_request={}&withdrawal_address={}",
        encode(&tx_json),
        encode(&destination_address.to_string())
    );
    let withdrawal_ui_url = format!("{}{}", config.get_withdrawal_sign_url(), query);

    Ok((WithdrawalUrl(withdrawal_ui_url), TxJson(tx_json), params))
}

pub async fn send_safe_withdrawal(
    params: SafeWithdrawalParams,
    secret_key: crate::secure_types::SecureString,
    config: &BridgeCliConfig,
) -> Result<TransactionReceipt, BridgeCliError> {
    // Parse the secret key with improved error handling
    let signer = secret_key
        .expose_secret()
        .parse::<PrivateKeySigner>()
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Invalid SECRET_KEY format: {}", e)))?;
    let chain_id: u64 = config.citrea_chain_id;
    let key = signer.with_chain_id(Some(chain_id));
    let wallet_address = key.address();

    tracing::debug!("Wallet address: {}", wallet_address);

    validate_address_purpose(&params.signer_address, Purpose::Withdrawal)?;

    let payout_output = TxOut {
        value: params.withdrawal_amount,
        script_pubkey: params.destination_address.script_pubkey(),
    };

    // verify signature
    verify_withdrawal_signature(
        &params.signature,
        &params.signer_address.address,
        &params.withdrawal_outpoint,
        &params.destination_address,
        params.withdrawal_amount,
        config,
    )?;

    let withdrawal_params = prepare_withdrawal_params(
        &params.withdrawal_outpoint,
        &payout_output,
        &params.signature,
        config,
    )
    .await?;

    let contract = create_bridge_contract(key, config)?;

    let citrea_withdrawal_tx = contract
        .safeWithdraw(
            withdrawal_params.transaction,
            withdrawal_params.merkle_proof,
            withdrawal_params.payout_transaction,
            withdrawal_params.block_header,
            withdrawal_params.output_script_pk,
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

pub(crate) async fn start_withdrawal(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    destination_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    if is_wallet_address(destination_address, config).await? {
        return Err(BridgeCliError::DestinationAddressIsWalletAddress);
    }
    validate_address_purpose(signer_address, Purpose::Withdrawal)?;
    Ok(())
}

pub async fn scan_withdrawal(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    _destination_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<Vec<crate::api_utils::UtxoInfo>, BridgeCliError> {
    validate_address_purpose(signer_address, Purpose::Withdrawal)?;

    let utxos = get_utxos(&signer_address.address, config).await?;

    Ok(utxos)
}

/// Common withdrawal parameter preparation pattern (reduces major duplication)  
pub async fn prepare_withdrawal_params(
    withdrawal_outpoint: &OutPoint,
    payout_output: &TxOut,
    sig: &bitcoin::taproot::Signature,
    config: &BridgeCliConfig,
) -> Result<WithdrawalParams, BridgeCliError> {
    if !is_tx_on_chain(&withdrawal_outpoint.txid, config).await? {
        return Err(BridgeCliError::TransactionNotOnChain(
            withdrawal_outpoint.txid,
        ));
    }

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
