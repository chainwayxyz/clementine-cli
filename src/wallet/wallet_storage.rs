//! Secure wallet data storage and registry management for Clementine CLI.
//!
//! This module handles persistent storage of encrypted wallet data and maintains
//! a centralized registry of all wallets:
//! - Storing encrypted wallet data with secure file permissions
//! - Managing a centralized wallet registry (wallets.json)
//! - Loading and retrieving stored wallet information
//! - Handling wallet file operations and directory management
//! - Supporting both generated and imported wallet workflows
//!
//! ## Storage Structure
//!
//! Wallets are stored in the `~/.clementine/keys/` directory:
//! - **Individual wallet files**: `wallet_{address}.json` containing encrypted data
//! - **Registry file**: `wallets.json` containing metadata for all wallets
//! - **Secure permissions**: Unix file permissions set to 0o600 (owner read/write only)
//!
//! ## Data Structures
//!
//! - [`WalletRegistryEntry`]: Metadata stored in the centralized registry
//! - [`GenericWalletData`]: Complete wallet data with encrypted secrets
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
use std::path::PathBuf;

use crate::errors::BridgeCliError;
use crate::sqlite_db::sqlite_client::SqliteDb;
use crate::sqlite_db::wallet_db::WalletData;
use crate::structs::{AddrDisplay, TaprootAddressWithPrefix};
use crate::wallet::encryption::{EncryptedData, encrypted_data_to_hex};
use crate::wallet::wallet_utils::{WalletValidationMode, validate_wallet_availability};
use crate::{get_clementine_home_dir, sqlite_db};

/// Generic function to store encrypted wallet data
#[allow(clippy::too_many_arguments)]
pub(crate) async fn store_wallet_data(
    address: &TaprootAddressWithPrefix<NetworkChecked>,
    network: Network,
    encrypted_mnemonic: &EncryptedData,
    encrypted_private_key: &EncryptedData,
    imported: bool,
    import_method: Option<&str>,
    label: &str,
) -> Result<(), BridgeCliError> {
    validate_wallet_availability(Some(label), Some(address), WalletValidationMode::Both)?;

    let wallet_data = WalletData {
        label: label.to_string(),
        address: address.clone(),
        network: network,
        encrypted_mnemonic: Some(encrypted_data_to_hex(encrypted_mnemonic)),
        encrypted_private_key: Some(encrypted_data_to_hex(encrypted_private_key)),
        created_at: chrono::Utc::now(),
        encryption_method: "aes256_gcm_argon2id_secure".to_string(),
        imported: if imported { Some(true) } else { None },
        import_method: import_method.map(|s| s.to_string()),
    };

    let sqlite_client = SqliteDb::open_with_schema().await?;
    sqlite_db::wallet_db::WalletTable::insert_wallet(&sqlite_client.pool(), &wallet_data).await?;

    Ok(())
}

/// Load generic wallet data from file
pub async fn load_wallet_data<T>(
    address: &TaprootAddressWithPrefix<T>,
) -> Result<Option<WalletData>, BridgeCliError>
where
    T: NetworkValidation + Clone,
    bitcoin::Address<T>: AddrDisplay,
{
    let sqlite_client = SqliteDb::open_with_schema().await?;
    let wallet_data = sqlite_db::wallet_db::WalletTable::get_wallet_by_address(
        &sqlite_client.pool(),
        address.clone(),
    )
    .await?;

    Ok(wallet_data)
}

/// Get the storage directory path
pub(crate) fn get_storage_dir() -> Result<PathBuf, BridgeCliError> {
    let home_dir = get_clementine_home_dir()?;
    Ok(home_dir.join("keys"))
}

pub(crate) fn get_storage_dir_with_existence_check() -> Result<PathBuf, BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    if !storage_dir.exists() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Storage directory does not exist: {}, please run 'clementine-cli init' to create it.",
            storage_dir.display()
        )));
    }
    Ok(storage_dir)
}

/// Copy a wallet file to a destination, creating parent directories if needed.
pub(crate) fn copy_wallet_file_to_destination(
    address: &TaprootAddressWithPrefix<NetworkUnchecked>,
    destination_path: &Path,
) -> Result<std::path::PathBuf, BridgeCliError> {
    let wallet_file_name = format!("wallet_{}.json", address.address_without_prefix());
    let storage_dir = get_storage_dir_with_existence_check()?;
    let wallet_file = storage_dir.join(wallet_file_name.clone());

    // If destination is a directory, create the filename
    let final_dest = if destination_path.is_dir() {
        destination_path.join(wallet_file_name)
    } else {
        destination_path.to_path_buf()
    };

    // Create parent directories if they don't exist
    if let Some(parent) = final_dest.parent() {
        fs::create_dir_all(parent)?;
    }

    // Copy the wallet file
    fs::copy(&wallet_file, &final_dest)?;

    Ok(final_dest)
}
