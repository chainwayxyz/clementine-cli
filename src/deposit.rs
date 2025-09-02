// Deposit-related commands and logic for Clementine CLI

use crate::api_utils::{get_tx_details, get_txout_details};
use crate::backend::create_deposit_account;
use crate::bitcoin_utils::{calculate_deposit_address, convert_btc_to_amount};
use crate::config::BridgeCliConfig;
use crate::errors::BridgeCliError;
use crate::parameters::get_citrea_deposit_params;
use crate::structs::TaprootAddressWithPrefix;
use crate::wallet::Purpose;
use crate::wallet::wallet_utils::ensure_wallet_exists;
use crate::wallet::wallet_utils::{load_key_with_purpose_check, validate_address_purpose};
use crate::{BitcoinAddress, CitreaAddress};
use bitcoin::{Amount, FeeRate, OutPoint, Transaction, Txid};
use eyre::Result;

/// Parameters for creating a signed recovery transaction
#[derive(Debug)]
pub struct RecoveryTxParams {
    pub citrea_addr: CitreaAddress,
    pub recovery_taproot_address: TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    pub outpoint: OutPoint,
    pub claim_addr: BitcoinAddress,
    pub fee_rate: Option<u64>,
    pub amount: Option<f64>,
}

/// Parameters for verifying a recovery transaction
#[derive(Debug)]
pub struct VerifyRecoveryTxParams {
    pub recovery_tx: Transaction,
    pub citrea_address: CitreaAddress,
    pub recovery_taproot_address: TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    pub amount: Option<f64>,
}

pub(crate) enum DepositStatusEnum {
    New,
    InProgress,
    Completed,
    Unknown,
}

impl DepositStatusEnum {
    pub(crate) fn from_backend_status(status: &str) -> Self {
        match status {
            "new" => DepositStatusEnum::New,
            "minted" => DepositStatusEnum::Completed,
            "flushing_initiating" | "flushing_initiated" | "flushing_broadcasting" | "sent" => {
                DepositStatusEnum::InProgress
            }
            _ => DepositStatusEnum::Unknown,
        }
    }
    pub fn as_string(&self) -> String {
        match self {
            DepositStatusEnum::New => "New".to_string(),
            DepositStatusEnum::InProgress => "In Progress".to_string(),
            DepositStatusEnum::Completed => "Completed".to_string(),
            DepositStatusEnum::Unknown => "Unknown".to_string(),
        }
    }
}

/// Get deposit address from backend
pub async fn get_deposit_address(
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    config: &BridgeCliConfig,
) -> Result<BitcoinAddress, BridgeCliError> {
    crate::wallet::wallet_utils::validate_address_purpose(
        recovery_taproot_address,
        Purpose::Deposit,
    )?;

    let (calculated_deposit_address, _) =
        calculate_deposit_address(citrea_address, &recovery_taproot_address.address, config)?;

    // Because backend is not available for regtest, don't cross check.
    if config.network == bitcoin::Network::Regtest {
        tracing::debug!("Regtest network is being used, not checking address against backend...");
        return Ok(calculated_deposit_address);
    }

    // Call backend to create deposit account
    let deposit_address =
        create_deposit_account(citrea_address, &recovery_taproot_address.address, config).await?;
    tracing::info!("Deposit address fetched from backend: {}", deposit_address);

    if deposit_address != calculated_deposit_address {
        return Err(BridgeCliError::CalculatedRecoveryTaprootAddressMismatch(
            calculated_deposit_address,
            deposit_address,
        ));
    }

    Ok(calculated_deposit_address)
}

pub async fn get_deposit_params(
    move_to_vault_txid: &Txid,
    config: &BridgeCliConfig,
) -> Result<Vec<u8>, BridgeCliError> {
    // 2. Get the prepare tx details
    let (move_to_vault_tx, move_to_vault_block, move_to_vault_block_height) =
        get_tx_details(move_to_vault_txid, config).await?;

    let move_to_vault_txout = get_txout_details(
        config,
        &move_to_vault_tx.input[0].previous_output.txid,
        move_to_vault_tx.input[0].previous_output.vout,
    )
    .await?;

    let deposit_params = get_citrea_deposit_params(
        move_to_vault_txout,
        &move_to_vault_tx,
        &move_to_vault_block,
        move_to_vault_block_height,
    )?;

    Ok(deposit_params)
}

/// Creates a signed raw transaction that can collect unminted funds from the
/// deposit transaction after 200 blocks.
pub fn create_signed_recovery_tx(
    params: RecoveryTxParams,
    config: &BridgeCliConfig,
) -> Result<Transaction, BridgeCliError> {
    ensure_wallet_exists(&params.recovery_taproot_address)?;
    // Always prompt for passphrase for maximum security
    let keypair = load_key_with_purpose_check(&params.recovery_taproot_address, Purpose::Deposit)?;

    // Convert BTC amount to satoshis if provided
    let deposit_amount = convert_btc_to_amount(params.amount)?;

    let fee_rate = params.fee_rate
        .map(FeeRate::from_sat_per_vb_unchecked)
        .unwrap_or_else(|| FeeRate::from_sat_per_vb_unchecked(10)); // Default 10 sat/vbyte

    let signed_tx = crate::bitcoin_utils::sign_recovery_tx(
        &keypair,
        &params.citrea_addr,
        &params.recovery_taproot_address.address,
        &params.outpoint,
        deposit_amount.unwrap_or(config.bridge_amount),
        &params.claim_addr,
        fee_rate,
        config,
    )?;

    Ok(signed_tx)
}

pub fn verify_recovery_tx(
    params: VerifyRecoveryTxParams,
    config: &BridgeCliConfig,
) -> Result<(Txid, BitcoinAddress, Amount), BridgeCliError> {
    validate_address_purpose(&params.recovery_taproot_address, Purpose::Deposit)?;

    let (txid, address, amount) = crate::bitcoin_utils::verify_recovery_tx(
        &params.recovery_tx,
        &params.citrea_address,
        &params.recovery_taproot_address.address,
        convert_btc_to_amount(params.amount)?,
        config,
    )?;

    Ok((txid, address, amount))
}
