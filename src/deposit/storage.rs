use crate::errors::BridgeCliError;
use crate::sqlite_db::deposit_db::{DepositRecord, DepositTable};
use crate::sqlite_db::sqlite_client::SqliteDb;
use crate::structs::TaprootAddressWithPrefix;
use crate::{BitcoinAddress, CitreaAddress};
use bitcoin::Network;
use bitcoin::address::NetworkUnchecked;
use bitcoin::secp256k1::XOnlyPublicKey;
use std::time::{SystemTime, UNIX_EPOCH};

/// Data needed to store a deposit address.
#[derive(Debug, Clone)]
pub struct DepositData {
    pub deposit_address: BitcoinAddress,
    pub recovery_taproot_address: TaprootAddressWithPrefix<NetworkUnchecked>,
    pub aggregated_public_key: XOnlyPublicKey,
    pub citrea_address: CitreaAddress,
    pub user_takes_after: u64,
    pub network: Network,
}

/// Public view of a stored deposit address.
#[derive(Debug, Clone)]
pub struct DepositAddressDetails {
    pub deposit_address: String,
    pub aggregated_public_key: String,
    pub recovery_taproot_address: String,
    pub citrea_address: String,
    pub user_takes_after: u64,
    pub network: Network,
    pub created_at: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DepositAddressStorageResult {
    Exists,
    NewlyStored,
}

/// Store a deposit address and its metadata.
///
/// If the address already exists, the existing record is kept.
/// Returns an error if the database cannot be read or written.
pub async fn store_deposit_address(
    deposit_data: &DepositData,
) -> Result<DepositAddressStorageResult, BridgeCliError> {
    let sqlite_client = SqliteDb::open_with_schema().await?;
    let deposit_address = deposit_data.deposit_address.to_string();

    if DepositTable::deposit_exists(sqlite_client.pool(), &deposit_address).await? {
        return Ok(DepositAddressStorageResult::Exists);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_else(|e| {
            tracing::warn!("System time error, using 0 as timestamp: {}", e);
            0
        });

    let record = DepositRecord {
        deposit_address,
        aggregated_public_key: deposit_data.aggregated_public_key.to_string(),
        recovery_taproot_address: deposit_data.recovery_taproot_address.address_with_prefix(),
        citrea_address: deposit_data.citrea_address.to_string(),
        user_takes_after: deposit_data.user_takes_after,
        network: deposit_data.network,
        created_at: now,
    };

    DepositTable::insert_deposit(sqlite_client.pool(), &record).await?;

    Ok(DepositAddressStorageResult::NewlyStored)
}

/// List all stored deposits.
///
/// Returns an empty vector if no deposits have been stored.
pub async fn get_all_deposit_address_details() -> Result<Vec<DepositAddressDetails>, BridgeCliError>
{
    let sqlite_client = SqliteDb::open_with_schema().await?;
    let records = DepositTable::get_all_deposits(sqlite_client.pool()).await?;

    Ok(records
        .into_iter()
        .map(|record| DepositAddressDetails {
            deposit_address: record.deposit_address,
            aggregated_public_key: record.aggregated_public_key,
            recovery_taproot_address: record.recovery_taproot_address,
            citrea_address: record.citrea_address,
            user_takes_after: record.user_takes_after,
            network: record.network,
            created_at: record.created_at,
        })
        .collect())
}

/// Retrieves full deposit details for a specific deposit address.
///
/// The deposit address is treated as a string key; the function does not
/// perform any additional validation beyond lookup.
///
/// Returns `Ok(Some(..))` if the deposit address exists in storage, or
/// `Ok(None)` if it is not found.
pub async fn get_deposit_address_details_for_deposit_address(
    deposit_address: &str,
) -> Result<Option<DepositAddressDetails>, BridgeCliError> {
    let sqlite_client = SqliteDb::open_with_schema().await?;
    let record =
        DepositTable::get_deposit_by_address(sqlite_client.pool(), deposit_address).await?;

    Ok(record.map(|record| DepositAddressDetails {
        deposit_address: record.deposit_address,
        aggregated_public_key: record.aggregated_public_key,
        recovery_taproot_address: record.recovery_taproot_address,
        citrea_address: record.citrea_address,
        user_takes_after: record.user_takes_after,
        network: record.network,
        created_at: record.created_at,
    }))
}
