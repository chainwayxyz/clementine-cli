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
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::{collections::HashMap, path::PathBuf};

use crate::errors::BridgeCliError;
use crate::get_clementine_home_dir;
use crate::structs::{AddrDisplay, TaprootAddressWithPrefix};
use crate::wallet::encryption::{EncryptedData, EncryptedDataHex, encrypted_data_to_hex};
use crate::wallet::wallet_utils::{WalletValidationMode, validate_wallet_availability};

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
) -> Result<(), BridgeCliError> {
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

    let storage_dir = get_storage_dir()?;
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

    Ok(())
}

/// Update the wallets.json registry
fn update_wallets_registry(
    label: &str,
    address: &TaprootAddressWithPrefix<NetworkChecked>,
    network: Network,
    imported: bool,
    import_method: Option<&str>,
) -> Result<(), BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    let wallets_file = storage_dir.join("wallets.json");

    let mut wallets: HashMap<String, WalletRegistryEntry> = if wallets_file.exists() {
        serde_json::from_str(&fs::read_to_string(&wallets_file)?)?
    } else {
        HashMap::new()
    };

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

    wallets.insert(address.address_without_prefix(), wallet_entry);
    fs::write(&wallets_file, serde_json::to_string_pretty(&wallets)?)?;

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
    let storage_dir = get_storage_dir()?;
    let address = address.address_without_prefix();
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    if !wallet_file.exists() {
        return Err(BridgeCliError::WalletNotFound(address));
    }

    let json_data = fs::read_to_string(&wallet_file).map_err(|e| {
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to read wallet file '{}': {}",
            wallet_file.display(),
            e
        ))
    })?;

    let wallet_data: GenericWalletData = serde_json::from_str(&json_data).map_err(|e| {
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to parse wallet file '{}': {}",
            wallet_file.display(),
            e
        ))
    })?;

    Ok(wallet_data)
}

/// Get the storage directory path
pub(crate) fn get_storage_dir() -> Result<PathBuf, BridgeCliError> {
    let home_dir = get_clementine_home_dir()?;
    Ok(home_dir.join("keys"))
}

/// Get wallets from the registry (wallets.json)
pub(crate) fn get_wallets_from_registry()
-> Result<HashMap<String, WalletRegistryEntry>, BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    let wallets_file = storage_dir.join("wallets.json");

    if !wallets_file.exists() {
        tracing::debug!("No wallets in the registry");
        return Ok(HashMap::new());
    }

    let wallets_content = fs::read_to_string(&wallets_file)
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Failed to read wallets registry: {}", e)))?;

    let wallets: HashMap<String, WalletRegistryEntry> = serde_json::from_str(&wallets_content)
        .map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!("Failed to parse wallets registry JSON: {}", e))
        })?;

    Ok(wallets)
}

/// Copy a wallet file to a destination, creating parent directories if needed.
pub(crate) fn copy_wallet_file_to_destination(
    address: &TaprootAddressWithPrefix<NetworkUnchecked>,
    destination_path: &Path,
) -> Result<std::path::PathBuf, BridgeCliError> {
    let wallet_file_name = format!("wallet_{}.json", address.address_without_prefix());
    let storage_dir = get_storage_dir()?;
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
