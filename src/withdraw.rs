// Withdrawal-related commands and logic for Clementine CLI

use crate::BitcoinAddress;
use crate::api_utils::{get_tx_details, get_utxos, is_tx_on_chain};
use crate::bitcoin_utils::{sign_withdrawal_signature, verify_withdrawal_signature};
use crate::config::BridgeCliConfig;
use crate::errors::BridgeCliError;
use crate::secure_types::SecureKeypair;
use crate::structs::TaprootAddressWithPrefix;
use crate::types::{BRIDGE_CONTRACT, CitreaContract, encode_safe_withdraw_params};
use crate::wallet::Purpose;
use crate::wallet::wallet_utils::{address_exists, ensure_wallet_exists, validate_address_purpose};
use alloy::eips::{BlockId, BlockNumberOrTag};
use alloy::network::EthereumWallet;
use alloy::primitives::U256;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::TransactionReceipt;
use alloy::signers::Signer;
use alloy::signers::local::PrivateKeySigner;
use bitcoin::hashes::Hash;
use bitcoin::taproot::Signature;
use bitcoin::{Amount, Network, OutPoint, TxOut, Txid};
use eyre::Context;
use serde_json::json;
use urlencoding::encode;

pub(crate) enum WithdrawStatusEnum {
    New,
    InProgress,
    OptimisticPayoutFailed,
    Completed,
    Unknown,
}

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
    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(key))
        .connect_http(config.citrea_rpc_url.clone());

    let contract = BRIDGE_CONTRACT::new(
        config
            .bridge_contract_address
            .parse()
            .wrap_err("Failed to parse bridge contract address")?,
        provider,
    );

    Ok(contract)
}

/// Helper function to securely load environment variable with better error handling
fn get_secret_key_from_env() -> Result<PrivateKeySigner, BridgeCliError> {
    let secret_key = std::env::var("SECRET_KEY")
        .map_err(|_| BridgeCliError::Eyre(eyre::eyre!("SECRET_KEY environment variable not found. Please set SECRET_KEY to proceed with this operation")))?;

    secret_key
        .parse::<PrivateKeySigner>()
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Invalid SECRET_KEY format: {}", e)))
}

pub fn generate_withdrawal_signatures(
    keypair: SecureKeypair,
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    destination_address: &BitcoinAddress,
    withdrawal_utxo: &OutPoint,
    optimistic_withdrawal_amount: &Amount,
    operator_withdrawal_amount: &Amount,
    network: Network,
) -> Result<(Signature, Signature), BridgeCliError> {
    ensure_wallet_exists(signer_address)?;

    if signer_address.purpose != Purpose::Withdrawal {
        return Err(BridgeCliError::PurposeMismatch {
            expected: Purpose::Withdrawal,
            found: signer_address.purpose,
        });
    }

    // If the claim address is a Taproot address, ensure it is not a Clementine wallet address
    if destination_address.address_type() == Some(bitcoin::AddressType::P2tr) {
        let claim_wallet_address = TaprootAddressWithPrefix::from_string_without_prefix(
            &destination_address.to_string(),
            Purpose::Withdrawal,
            network,
        )?;

        // Check if the claim address belongs to any of our wallets
        if address_exists(&claim_wallet_address)? {
            return Err(BridgeCliError::DestinationAddressIsWalletAddress);
        }
    }

    let optimistic_withdrawal_signature = sign_withdrawal_signature(
        &keypair,
        &signer_address.address,
        withdrawal_utxo,
        destination_address,
        *optimistic_withdrawal_amount,
    )?;

    let operator_withdrawal_signature = sign_withdrawal_signature(
        &keypair,
        &signer_address.address,
        withdrawal_utxo,
        destination_address,
        *operator_withdrawal_amount,
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
) -> Result<String, BridgeCliError> {
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
    )?;

    let params =
        prepare_withdrawal_params(withdrawal_outpoint, &payout_output, sig, config).await?;

    let calldata_hex =
        encode_safe_withdraw_params(&params.0, &params.1, &params.2, params.3, params.4);

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

/// Checks every UTXO in the contract and finds the index for it.
pub async fn get_withdrawal_index(
    withdrawal_utxo: OutPoint,
    config: &BridgeCliConfig,
) -> Result<u32, BridgeCliError> {
    let signer = get_secret_key_from_env().unwrap_or(PrivateKeySigner::random());
    let chain_id: u64 = config.citrea_chain_id;
    let key = signer.with_chain_id(Some(chain_id));
    let contract = create_bridge_contract(key, config)?;

    let withdrawal_count = contract
        .getWithdrawalCount()
        .block(BlockId::Number(BlockNumberOrTag::Latest))
        .call()
        .await
        .wrap_err("Can't get withdrawal count")?;
    let withdrawal_count: u32 = withdrawal_count
        .try_into()
        .wrap_err("Can't convert withdrawal count")?;
    tracing::debug!("Current withdrawal count: {}", withdrawal_count);

    for i in (0..withdrawal_count).rev() {
        let contract_withdrawal_utxo = contract
            .withdrawalUTXOs(U256::from(i))
            .call()
            .await
            .wrap_err("Can't get withdrawal UTXO")?;
        tracing::debug!("Received withdrawal UTXO {:?}", contract_withdrawal_utxo);

        let txid = contract_withdrawal_utxo._0;
        let txid = Txid::from_slice(txid.as_ref()).wrap_err("Failed to convert txid to Txid")?;
        let vout = contract_withdrawal_utxo._1;
        let vout = u32::from_le_bytes(*vout);
        let utxo = OutPoint { txid, vout };

        if utxo == withdrawal_utxo {
            return Ok(i);
        }
    }

    Err(BridgeCliError::CantFindUTXO(withdrawal_utxo))
}

pub(crate) fn start_withdrawal(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    _destination_address: &BitcoinAddress,
    _config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    validate_address_purpose(signer_address, Purpose::Withdrawal)?;
    Ok(())
}

pub async fn scan_withdrawal(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    _destination_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<Vec<(OutPoint, Amount)>, BridgeCliError> {
    validate_address_purpose(signer_address, Purpose::Withdrawal)?;

    let utxos = get_utxos(&signer_address.address, config).await?;
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
    if !is_tx_on_chain(&withdrawal_outpoint.txid, config).await? {
        return Err(BridgeCliError::TransactionNotOnChain(
            withdrawal_outpoint.txid,
        ));
    }

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
