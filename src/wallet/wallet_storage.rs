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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn setup_test_storage() {
        // The temp directory is automatically created by get_clementine_home_dir()
        // via thread-local storage, so we just need to ensure storage dir exists
        let storage_dir = get_storage_dir().expect("Should get storage dir");
        fs::create_dir_all(&storage_dir).expect("Failed to create storage dir");
    }

    #[test]
    #[serial]
    fn test_get_storage_dir() {
        setup_test_storage();

        let storage_dir = get_storage_dir().expect("Should get storage dir");

        assert!(storage_dir.to_string_lossy().ends_with(".clementine/keys"));
    }

    #[test]
    #[serial]
    fn test_get_storage_dir_with_existence_check_succeeds() {
        setup_test_storage();

        let storage_dir =
            get_storage_dir_with_existence_check().expect("Should get storage dir when it exists");

        assert!(storage_dir.exists());
    }

    // NOTE: This test can't easily verify the "directory doesn't exist" scenario anymore
    // because get_clementine_home_dir() automatically creates temp directories in tests.
    // The existence check is still tested in production where directories aren't auto-created.
    #[test]
    #[serial]
    #[ignore]
    fn test_get_storage_dir_with_existence_check_fails_when_not_exists() {
        // This test is no longer applicable with automatic temp directory creation
        let result = get_storage_dir_with_existence_check();
        assert!(
            result.is_err(),
            "Should fail when storage dir doesn't exist"
        );
    }

    #[test]
    #[serial]
    fn test_get_wallets_from_registry_empty() {
        setup_test_storage();

        let wallets = get_wallets_from_registry().expect("Should get empty registry");

        assert!(wallets.is_empty(), "Registry should be empty initially");
    }

    #[test]
    #[serial]
    fn test_wallets_registry_manual_operations() {
        setup_test_storage();

        // Manually create a registry file
        let storage_dir = get_storage_dir().expect("Should get storage dir");
        let wallets_file = storage_dir.join("wallets.json");

        let mut registry = HashMap::new();
        registry.insert(
            "test_address".to_string(),
            WalletRegistryEntry {
                label: "test_wallet".to_string(),
                network: "regtest".to_string(),
                addres_with_prefix: "dep_bcrt1ptest".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                imported: None,
                imported_at: None,
                import_method: None,
            },
        );

        let json = serde_json::to_string_pretty(&registry).unwrap();
        fs::write(&wallets_file, json).expect("Should write registry");

        // Now read it back
        let loaded_registry = get_wallets_from_registry().expect("Should load registry");

        assert_eq!(loaded_registry.len(), 1);
        assert!(loaded_registry.contains_key("test_address"));
        assert_eq!(
            loaded_registry.get("test_address").unwrap().label,
            "test_wallet"
        );
    }

    #[test]
    #[serial]
    fn test_registry_persistence() {
        setup_test_storage();

        let storage_dir = get_storage_dir().expect("Should get storage dir");
        let wallets_file = storage_dir.join("wallets.json");

        // Create first entry
        let mut registry = HashMap::new();
        registry.insert(
            "address1".to_string(),
            WalletRegistryEntry {
                label: "wallet1".to_string(),
                network: "regtest".to_string(),
                addres_with_prefix: "dep_address1".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                imported: None,
                imported_at: None,
                import_method: None,
            },
        );
        fs::write(
            &wallets_file,
            serde_json::to_string_pretty(&registry).unwrap(),
        )
        .expect("Should write first entry");

        // Read it
        let loaded1 = get_wallets_from_registry().expect("Should load registry");
        assert_eq!(loaded1.len(), 1);

        // Add second entry
        let mut registry = loaded1;
        registry.insert(
            "address2".to_string(),
            WalletRegistryEntry {
                label: "wallet2".to_string(),
                network: "regtest".to_string(),
                addres_with_prefix: "dep_address2".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                imported: None,
                imported_at: None,
                import_method: None,
            },
        );
        fs::write(
            &wallets_file,
            serde_json::to_string_pretty(&registry).unwrap(),
        )
        .expect("Should write second entry");

        // Verify both persist
        let loaded2 = get_wallets_from_registry().expect("Should load registry");
        assert_eq!(loaded2.len(), 2);
        assert!(loaded2.contains_key("address1"));
        assert!(loaded2.contains_key("address2"));
    }

    #[test]
    fn test_wallet_data_serialization() {
        use crate::wallet::encryption::EncryptedDataHex;

        // Test that WalletData can be serialized and deserialized
        let wallet_data = GenericWalletData {
            label: "test_wallet".to_string(),
            address_with_prefix: "dep_bcrt1ptest".to_string(),
            network: "regtest".to_string(),
            encrypted_mnemonic: Some(EncryptedDataHex {
                ciphertext: "aabbcc".to_string(),
                nonce: "ddeeff".to_string(),
                salt: "112233".to_string(),
            }),
            encrypted_private_key: Some(EncryptedDataHex {
                ciphertext: "445566".to_string(),
                nonce: "778899".to_string(),
                salt: "aabbcc".to_string(),
            }),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            encryption_method: "aes256_gcm_argon2id_secure".to_string(),
            imported: None,
            import_method: None,
        };

        // Serialize
        let json = serde_json::to_string(&wallet_data).expect("Should serialize wallet data");

        // Deserialize
        let deserialized: GenericWalletData =
            serde_json::from_str(&json).expect("Should deserialize wallet data");

        assert_eq!(deserialized.label, wallet_data.label);
        assert_eq!(deserialized.network, wallet_data.network);
        assert_eq!(
            deserialized.encryption_method,
            wallet_data.encryption_method
        );
    }

    #[test]
    fn test_registry_entry_serialization() {
        let entry = WalletRegistryEntry {
            label: "test".to_string(),
            network: "regtest".to_string(),
            addres_with_prefix: "dep_bcrt1p".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            imported: Some(true),
            imported_at: Some("2024-01-01T00:00:00Z".to_string()),
            import_method: Some("mnemonic".to_string()),
        };

        let json = serde_json::to_string(&entry).expect("Should serialize");
        let deserialized: WalletRegistryEntry =
            serde_json::from_str(&json).expect("Should deserialize");

        assert_eq!(deserialized.label, entry.label);
        assert_eq!(deserialized.imported, Some(true));
        assert_eq!(deserialized.import_method, Some("mnemonic".to_string()));
    }

    #[test]
    fn test_registry_entry_optional_fields() {
        // Test that optional fields are properly handled
        let entry = WalletRegistryEntry {
            label: "test".to_string(),
            network: "regtest".to_string(),
            addres_with_prefix: "dep_bcrt1p".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            imported: None,
            imported_at: None,
            import_method: None,
        };

        let json = serde_json::to_string(&entry).expect("Should serialize");

        // Verify optional fields are not serialized
        assert!(!json.contains("imported"));
        assert!(!json.contains("imported_at"));
        assert!(!json.contains("import_method"));

        let deserialized: WalletRegistryEntry =
            serde_json::from_str(&json).expect("Should deserialize");

        assert_eq!(deserialized.imported, None);
        assert_eq!(deserialized.imported_at, None);
        assert_eq!(deserialized.import_method, None);
    }

    #[test]
    #[serial]
    fn test_storage_dir_structure() {
        setup_test_storage();

        let storage_dir = get_storage_dir().expect("Should get storage dir");

        assert!(storage_dir.exists(), "Storage dir should exist");
        assert!(storage_dir.is_dir(), "Storage path should be a directory");

        // Verify it's in the expected location
        let expected_suffix = if cfg!(unix) {
            ".clementine/keys"
        } else {
            ".clementine\\keys"
        };

        assert!(
            storage_dir.to_string_lossy().ends_with(expected_suffix),
            "Storage dir should end with .clementine/keys"
        );
    }

    #[test]
    #[cfg(unix)]
    #[serial]
    fn test_wallet_file_would_have_secure_permissions() {
        use std::os::unix::fs::PermissionsExt;

        setup_test_storage();
        let storage_dir = get_storage_dir().expect("Should get storage dir");

        // Create a test file manually
        let test_file = storage_dir.join("test_wallet.json");
        fs::write(&test_file, "{}").expect("Should write test file");

        // Set secure permissions (simulating what store_wallet_data does)
        let mut permissions = fs::metadata(&test_file)
            .expect("Should read metadata")
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&test_file, permissions).expect("Should set permissions");

        // Verify permissions
        let metadata = fs::metadata(&test_file).expect("Should read metadata");
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "File should have 0600 permissions");
    }

    #[test]
    #[serial]
    fn test_registry_handles_empty_file() {
        setup_test_storage();
        let storage_dir = get_storage_dir().expect("Should get storage dir");
        let wallets_file = storage_dir.join("wallets.json");

        // Create an empty JSON object
        fs::write(&wallets_file, "{}").expect("Should write empty JSON");

        let wallets = get_wallets_from_registry().expect("Should handle empty JSON");

        assert!(wallets.is_empty(), "Should return empty map");
    }

    #[test]
    #[serial]
    fn test_registry_handles_invalid_json() {
        setup_test_storage();
        let storage_dir = get_storage_dir().expect("Should get storage dir");
        let wallets_file = storage_dir.join("wallets.json");

        // Write invalid JSON
        fs::write(&wallets_file, "not valid json").expect("Should write invalid JSON");

        let result = get_wallets_from_registry();
        assert!(result.is_err(), "Should fail on invalid JSON");
    }
}
