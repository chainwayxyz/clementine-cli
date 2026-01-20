//! Secure wallet data storage and registry management for Clementine CLI.
//!
//! This module handles persistent storage of encrypted wallet data and maintains
//! a centralized registry of all wallets using SQLite:
//! - Storing encrypted wallet data with secure file permissions
//! - Persisting wallet metadata and encrypted secrets in SQLite
//! - Loading and retrieving stored wallet information
//! - Exporting wallet backups to JSON files
//! - Supporting both generated and imported wallet workflows
//!
//! ## Data Structures
//!
//! - [`WalletData`]: Database model for stored wallet data
//! - [`WalletExport`]: Serialized wallet export format
//!
//! ## Encryption Standards
//!
//! All sensitive data is encrypted using:
//! - **Algorithm**: AES-256-GCM for authenticated encryption
//! - **Key derivation**: Argon2id for password-based key derivation
//! - **Secure handling**: Automatic zeroization of sensitive memory
//!
//! ## Import Support
//!
//! Tracks wallet creation methods:
//! - **Generated wallets**: Created from new mnemonic phrases
//! - **Imported wallets**: Imported from existing mnemonics or private keys
//! - **Import metadata**: Timestamps and import method tracking
//!

use bitcoin::Network;
use bitcoin::address::{NetworkChecked, NetworkUnchecked, NetworkValidation};
use std::fs;
use std::path::Path;

use crate::errors::BridgeCliError;
use crate::sqlite_db;
use crate::sqlite_db::sqlite_client::SqliteDb;
use crate::sqlite_db::sqlite_client::resolve_sqlite_client;
use crate::sqlite_db::wallet_db::{WalletData, WalletExport};
use crate::wallet::ImportMethod;
use crate::wallet::encryption::{EncryptedData, encrypted_data_to_hex};
use crate::wallet::wallet_utils::validate_wallet_availability;
use crate::wallet::{AddrDisplay, TaprootAddressWithPrefix};

/// Generic function to store encrypted wallet data
#[allow(clippy::too_many_arguments)]
pub(crate) async fn store_wallet_data(
    address: &TaprootAddressWithPrefix<NetworkChecked>,
    network: Network,
    encrypted_mnemonic: Option<&EncryptedData>,
    encrypted_private_key: &EncryptedData,
    imported: bool,
    original_import_method: Option<ImportMethod>,
    import_method: Option<ImportMethod>,
    label: &str,
    sqlite_client: Option<&SqliteDb>,
) -> Result<(), BridgeCliError> {
    let sqlite_client = resolve_sqlite_client(sqlite_client).await?;

    validate_wallet_availability(Some(label), Some(address), Some(sqlite_client.as_ref())).await?;

    let wallet_data = WalletData {
        label: label.to_string(),
        address: address.clone(),
        network,
        encrypted_mnemonic: encrypted_mnemonic.map(encrypted_data_to_hex),
        encrypted_private_key: encrypted_data_to_hex(encrypted_private_key),
        created_at: chrono::Utc::now(),
        encryption_method: "aes256_gcm_argon2id_secure".to_string(),
        imported,
        original_import_method,
        import_method,
    };

    sqlite_db::wallet_db::WalletTable::insert_wallet(sqlite_client.as_ref().pool(), &wallet_data)
        .await?;

    Ok(())
}

/// Load wallet data from database by address.
pub async fn load_wallet_data<T>(
    address: &TaprootAddressWithPrefix<T>,
    sqlite_client: Option<&SqliteDb>,
) -> Result<Option<WalletData>, BridgeCliError>
where
    T: NetworkValidation + Clone,
    bitcoin::Address<T>: AddrDisplay,
{
    let sqlite_client = resolve_sqlite_client(sqlite_client).await?;

    let wallet_data = sqlite_db::wallet_db::WalletTable::get_wallet_by_address(
        sqlite_client.as_ref().pool(),
        address.clone(),
    )
    .await?;

    Ok(wallet_data)
}

/// Export a wallet to JSON at the destination path.
pub(crate) async fn extract_wallet_data_to_file(
    address: &TaprootAddressWithPrefix<NetworkUnchecked>,
    destination_path: &Path,
    sqlite_client: Option<&SqliteDb>,
) -> Result<std::path::PathBuf, BridgeCliError> {
    let sqlite_client = resolve_sqlite_client(sqlite_client).await?;

    let wallet_file_name = format!("wallet_{}.json", address.address_without_prefix());

    let wallet_data: WalletExport = match sqlite_db::wallet_db::WalletTable::get_wallet_by_address(
        sqlite_client.as_ref().pool(),
        address.clone(),
    )
    .await?
    {
        Some(wallet_data) => WalletExport::from(&wallet_data),
        None => {
            return Err(BridgeCliError::Eyre(eyre::eyre!(
                "Wallet with address {} not found in database.",
                address.address_with_prefix()
            )));
        }
    };

    let final_dest = if destination_path.is_dir() {
        destination_path.join(wallet_file_name)
    } else {
        destination_path.to_path_buf()
    };

    if let Some(parent) = final_dest.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&final_dest, serde_json::to_string_pretty(&wallet_data)?).map_err(|e| {
        tracing::error!(
            "Failed to write wallet data to file {}: {}",
            final_dest.display(),
            e
        );
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to write wallet data to file {}",
            final_dest.display(),
        ))
    })?;

    Ok(final_dest)
}
