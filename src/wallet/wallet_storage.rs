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

use crate::errors::BridgeCliError;
use crate::sqlite_db;
use crate::sqlite_db::sqlite_client::SqliteDb;
use crate::sqlite_db::wallet_db::{WalletData, WalletExport};
use crate::structs::{AddrDisplay, TaprootAddressWithPrefix};
use crate::wallet::encryption::{EncryptedData, encrypted_data_to_hex};
use crate::wallet::wallet_utils::{WalletValidationMode, validate_wallet_availability};

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
    validate_wallet_availability(Some(label), Some(address), WalletValidationMode::Both).await?;

    let wallet_data = WalletData {
        label: label.to_string(),
        address: address.clone(),
        network,
        encrypted_mnemonic: Some(encrypted_data_to_hex(encrypted_mnemonic)),
        encrypted_private_key: Some(encrypted_data_to_hex(encrypted_private_key)),
        created_at: chrono::Utc::now(),
        encryption_method: "aes256_gcm_argon2id_secure".to_string(),
        imported: if imported { Some(true) } else { None },
        import_method: import_method.map(|s| s.to_string()),
    };

    let sqlite_client = SqliteDb::open_with_schema().await?;
    sqlite_db::wallet_db::WalletTable::insert_wallet(sqlite_client.pool(), &wallet_data).await?;

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
        sqlite_client.pool(),
        address.clone(),
    )
    .await?;

    Ok(wallet_data)
}

/// Export a wallet to JSON at the destination path.
pub(crate) async fn extract_wallet_data_to_file(
    address: &TaprootAddressWithPrefix<NetworkUnchecked>,
    destination_path: &Path,
) -> Result<std::path::PathBuf, BridgeCliError> {
    let wallet_file_name = format!("wallet_{}.json", address.address_without_prefix());

    let sqlite_client = SqliteDb::open_with_schema().await?;

    let wallet_data: WalletExport = match sqlite_db::wallet_db::WalletTable::get_wallet_by_address(
        sqlite_client.pool(),
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
