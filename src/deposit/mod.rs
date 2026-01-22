// Deposit-related commands and logic for Clementine CLI

mod params;
mod status;
pub(crate) mod storage;

pub type CitreaAddress = alloy::primitives::Address;

pub use params::{RecoveryTxParams, VerifyRecoveryTxParams};
pub use status::{DepositStatus, DepositStatusWithVout};
pub use storage::{
    DepositAddressDetails, DepositAddressStorageResult, get_all_deposit_address_details,
    get_deposit_address_details_for_deposit_address,
};

use crate::btc::utils::{calculate_deposit_address, convert_btc_to_amount};
use crate::core::config::BridgeCliConfig;
use crate::core::errors::BridgeCliError;
use crate::core::parameters::get_citrea_deposit_params;
use crate::core::secure_types::SecureKeypair;
use crate::services::api::{get_tx_details, get_txout_details};
use crate::services::backend::create_deposit_account;
use crate::sqlite_db::sqlite_client::SqliteDb;
use crate::wallet::BitcoinAddress;
use crate::wallet::Purpose;
use crate::wallet::TaprootAddressWithPrefix;
use crate::wallet::wallet_utils::{ensure_wallet_exists, validate_address_purpose};
use bitcoin::{Amount, FeeRate, Transaction, Txid};
use eyre::Result;
use std::str::FromStr;
use storage::DepositData;
use storage::store_deposit_address;

pub fn parse_citrea_address(citrea_address: &str) -> Result<CitreaAddress, BridgeCliError> {
    CitreaAddress::from_str(citrea_address)
        .map_err(|e| {
            tracing::error!("Invalid Citrea address format: {} Error: {}", citrea_address, e);
            BridgeCliError::Eyre(eyre::eyre!("Invalid Citrea address format: {}", citrea_address))
        })
}

/// Get deposit address from backend
pub async fn get_deposit_address(
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    config: &BridgeCliConfig,
    sqlite_client: Option<&SqliteDb>,
) -> Result<(BitcoinAddress, DepositAddressStorageResult), BridgeCliError> {
    crate::wallet::wallet_utils::validate_address_purpose(
        recovery_taproot_address,
        Purpose::Deposit,
    )?;

    let (calculated_deposit_address, _) =
        calculate_deposit_address(citrea_address, &recovery_taproot_address.address, config)?;

    // Because backend is not available for regtest, don't cross check.
    let calculated_deposit_address = if config.network == bitcoin::Network::Regtest {
        tracing::debug!("Regtest network is being used, not checking address against backend...");
        calculated_deposit_address
    } else {
        // Call backend to create deposit account
        let deposit_address =
            create_deposit_account(citrea_address, &recovery_taproot_address.address, config)
                .await?;
        tracing::info!("Deposit address fetched from backend: {}", deposit_address);

        if deposit_address != calculated_deposit_address {
            return Err(BridgeCliError::CalculatedRecoveryTaprootAddressMismatch(
                calculated_deposit_address,
                deposit_address,
            ));
        };
        deposit_address
    };

    let storage_result = store_deposit_record(
        &calculated_deposit_address,
        recovery_taproot_address,
        citrea_address,
        config,
        sqlite_client,
    )
    .await?;

    Ok((calculated_deposit_address, storage_result))
}

async fn store_deposit_record(
    deposit_address: &BitcoinAddress,
    recovery_taproot_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    citrea_address: &CitreaAddress,
    config: &BridgeCliConfig,
    sqlite_client: Option<&SqliteDb>,
) -> Result<DepositAddressStorageResult, BridgeCliError> {
    let deposit_data = DepositData {
        deposit_address: deposit_address.clone(),
        recovery_taproot_address: recovery_taproot_address.into(),
        aggregated_public_key: config.aggregated_public_key,
        citrea_address: *citrea_address,
        user_takes_after: config.user_takes_after,
        network: config.network,
    };

    store_deposit_address(&deposit_data, sqlite_client)
        .await
        .map_err(|e| {
            tracing::error!("Failed to store deposit address: {}", e);
            BridgeCliError::Eyre(eyre::eyre!(
                "Failed to store deposit address for recovery taproot address '{}'",
                recovery_taproot_address.address_with_prefix()
            ))
        })
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
pub async fn create_signed_recovery_tx(
    params: RecoveryTxParams,
    config: &BridgeCliConfig,
    keypair: SecureKeypair,
    sqlite_client: Option<&SqliteDb>,
) -> Result<Transaction, BridgeCliError> {
    ensure_wallet_exists(&params.recovery_taproot_address, sqlite_client).await?;

    // Convert BTC amount to satoshis if provided
    let deposit_amount = convert_btc_to_amount(params.amount)?;

    let fee_rate = params
        .fee_rate
        .map(FeeRate::from_sat_per_vb_unchecked)
        .unwrap_or_else(|| FeeRate::from_sat_per_vb_unchecked(10)); // Default 10 sat/vbyte

    let signed_tx = crate::btc::utils::sign_recovery_tx(
        &keypair,
        &params.citrea_addr,
        &params.recovery_taproot_address.address,
        &params.outpoint,
        deposit_amount.unwrap_or(config.bridge_amount),
        &params.destination_addr,
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

    let (txid, address, amount) = crate::btc::utils::verify_recovery_tx(
        &params.recovery_tx,
        &params.citrea_address,
        &params.recovery_taproot_address.address,
        convert_btc_to_amount(params.amount)?,
        config,
    )?;

    Ok((txid, address, amount))
}
