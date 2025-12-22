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

use bitcoin::address::{NetworkChecked, NetworkUnchecked, NetworkValidation};
use bitcoin::Network;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::{collections::HashMap, path::PathBuf};

use crate::errors::BridgeCliError;
use crate::get_clementine_home_dir;
use crate::structs::{AddrDisplay, TaprootAddressWithPrefix};
use crate::utils::storage_lock::AtomicFileStorage;
use crate::wallet::encryption::{EncryptedData, EncryptedDataHex, encrypted_data_to_hex};
use crate::wallet::wallet_utils::{WalletValidationMode, validate_wallet_availability};

const WALLETS_REGISTRY_FILE: &str = "wallets.json";
const WALLETS_REGISTRY_LOCK_FILE: &str = "wallets.lock";

/// Registry entry for a wallet stored in wallets.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WalletRegistryEntry {
    pub label: String,
    pub network: String,
    pub created_at: String,
    pub addres_with_prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_method: Option<String>,
}

/// Map key for a wallet entry.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub(crate) struct WalletRegistryKey(pub String);

type WalletRegistryMap = HashMap<WalletRegistryKey, WalletRegistryEntry>;

/// Generic wallet data structure that can handle different storage formats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GenericWalletData {
    pub label: String,
    pub address_with_prefix: String,
    pub network: String,
    pub encrypted_mnemonic: Option<EncryptedDataHex>,
    pub encrypted_private_key: Option<EncryptedDataHex>,
    pub created_at: String,
    pub encryption_method: String,
    pub imported: Option<bool>,
    pub import_method: Option<String>,
}

/// Generic function to store encrypted wallet data
#[allow(clippy::too_many_arguments)]
pub(crate) fn store_wallet_data(
    address: &TaprootAddressWithPrefix<NetworkChecked>,
    network: Network,
    encrypted_mnemonic: &EncryptedData,
    encrypted_private_key: &EncryptedData,
    imported: bool,
    import_method: Option<&str>,
    label: &str,
) -> Result<PathBuf, BridgeCliError> {
    validate_wallet_availability(Some(label), Some(address), WalletValidationMode::Both)?;

    let wallet_data = GenericWalletData {
        label: label.to_string(),
        address_with_prefix: address.address_with_prefix(),
        network: network.to_string(),
        encrypted_mnemonic: Some(encrypted_data_to_hex(encrypted_mnemonic)),
        encrypted_private_key: Some(encrypted_data_to_hex(encrypted_private_key)),
        created_at: chrono::Utc::now().to_rfc3339(),
        encryption_method: "aes256_gcm_argon2id_secure".to_string(),
        imported: if imported { Some(true) } else { None },
        import_method: import_method.map(|s| s.to_string()),
    };

    let storage_dir = get_storage_dir_with_existence_check()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address.address_without_prefix()));
    tracing::info!("Wallet will be saved to: {wallet_file:?}");

    let json_data = serde_json::to_string_pretty(&wallet_data)?;
    tracing::debug!("Wallet data: {wallet_data:?}");

    // Create missing dirs and write to file.
    fs::create_dir_all(storage_dir)?;
    fs::write(&wallet_file, json_data)?;

    // Set secure file permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&wallet_file)?.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&wallet_file, permissions)?;
    }

    // Update wallets registry
    update_wallets_registry(label, address, network, imported, import_method)?;

    Ok(wallet_file)
}

/// Update the wallets.json registry
fn update_wallets_registry(
    label: &str,
    address: &TaprootAddressWithPrefix<NetworkChecked>,
    network: Network,
    imported: bool,
    import_method: Option<&str>,
) -> Result<(), BridgeCliError> {
    let storage_dir = get_storage_dir_with_existence_check()?;
    let wallets_file = storage_dir.join(WALLETS_REGISTRY_FILE);
    let lock_file = storage_dir.join(WALLETS_REGISTRY_LOCK_FILE);

    let registry = AtomicFileStorage::<WalletRegistryMap>::new(
        wallets_file.clone(),
        lock_file.clone(),
    )?;

    let wallet_entry = WalletRegistryEntry {
        label: label.to_string(),
        network: network.to_string(),
        addres_with_prefix: address.address_with_prefix(),
        created_at: chrono::Utc::now().to_rfc3339(),
        imported: if imported { Some(true) } else { None },
        imported_at: if imported {
            Some(chrono::Utc::now().to_rfc3339())
        } else {
            None
        },
        import_method: if imported {
            import_method.map(|s| s.to_string())
        } else {
            None
        },
    };

    let wallet_key = WalletRegistryKey(address.address_without_prefix());

    registry.insert_exclusive(|map| {
        map.insert(wallet_key, wallet_entry);
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&wallets_file)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&wallets_file, perms)?;
    }

    Ok(())
}

/// Load generic wallet data from file
pub(crate) fn load_wallet_data<T>(
    address: &TaprootAddressWithPrefix<T>,
) -> Result<GenericWalletData, BridgeCliError>
where
    T: NetworkValidation,
    bitcoin::Address<T>: AddrDisplay,
{
    let storage_dir = get_storage_dir_with_existence_check()?;
    let address = address.address_without_prefix();
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    if !wallet_file.exists() {
        return Err(BridgeCliError::WalletNotFound(address));
    }

    let json_data = fs::read_to_string(&wallet_file).map_err(|e| {
        tracing::error!(
            "Error reading wallet file '{}': {}",
            wallet_file.display(),
            e
        );
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to read wallet file '{}'",
            wallet_file.display()
        ))
    })?;

    let wallet_data: GenericWalletData = serde_json::from_str(&json_data).map_err(|e| {
        tracing::error!(
            "Error parsing wallet file '{}': {}",
            wallet_file.display(),
            e
        );
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to parse wallet file '{}'",
            wallet_file.display()
        ))
    })?;

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

/// Get wallets from the registry (wallets.json)
pub(crate) fn get_wallets_from_registry()
-> Result<HashMap<String, WalletRegistryEntry>, BridgeCliError> {
    let storage_dir = get_storage_dir_with_existence_check()?;
    let wallets_file = storage_dir.join(WALLETS_REGISTRY_FILE);
    let lock_file = storage_dir.join(WALLETS_REGISTRY_LOCK_FILE);

    let registry = AtomicFileStorage::<WalletRegistryMap>::new(wallets_file, lock_file)?;

    let map = match registry.read_shared() {
        Ok(m) => m,
        Err(BridgeCliError::FileNotFound(_)) => {
            tracing::debug!("No wallets in the registry");
            return Ok(HashMap::new());
        }
        Err(e) => return Err(e),
    };

    Ok(map
        .into_iter()
        .map(|(WalletRegistryKey(address), entry)| (address, entry))
        .collect())
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
