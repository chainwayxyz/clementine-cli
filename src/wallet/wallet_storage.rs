use bitcoin::Network;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::fs;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use crate::errors::BridgeCliError;
use crate::get_clementine_home_dir;
use crate::wallet::encryption::{EncryptedData, EncryptedDataHex, encrypted_data_to_hex};
use crate::wallet::wallet_utils::{WalletValidationMode, validate_wallet_availability};

/// Registry entry for a wallet stored in wallets.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WalletRegistryEntry {
    pub address: String,
    pub network: String,
    pub created_at: String,
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
    pub wallet_name: String,
    pub address: String,
    pub network: String,
    pub encrypted_mnemonic: Option<EncryptedDataHex>,
    pub encrypted_private_key: Option<EncryptedDataHex>,
    pub created_at: String,
    pub encryption_method: String,
    pub data_format: String,
    pub imported: Option<bool>,
    pub import_method: Option<String>,
}

/// Generic function to store encrypted wallet data
#[allow(clippy::too_many_arguments)]
pub(crate) fn store_wallet_data(
    address: &str,
    network: Network,
    encrypted_mnemonic: &EncryptedData,
    encrypted_private_key: &EncryptedData,
    data_format: &str,
    imported: bool,
    import_method: Option<&str>,
    wallet_name: &str,
) -> Result<(), BridgeCliError> {
    validate_wallet_availability(
        Some(wallet_name),
        None,
        None,
        WalletValidationMode::WalletName,
    )?;

    let wallet_data = GenericWalletData {
        wallet_name: wallet_name.to_string(),
        address: address.to_string(),
        network: network.to_string(),
        encrypted_mnemonic: Some(encrypted_data_to_hex(encrypted_mnemonic)),
        encrypted_private_key: Some(encrypted_data_to_hex(encrypted_private_key)),
        created_at: chrono::Utc::now().to_rfc3339(),
        encryption_method: "aes256_gcm_argon2id_secure".to_string(),
        data_format: data_format.to_string(),
        imported: if imported { Some(true) } else { None },
        import_method: import_method.map(|s| s.to_string()),
    };

    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_name));

    // Write wallet file
    let json_data = serde_json::to_string_pretty(&wallet_data)?;
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
    update_wallets_registry(wallet_name, address, network, imported, import_method)?;

    println!(
        "Wallet data for '{}' stored successfully in '{}'",
        wallet_name.blue().bold(),
        wallet_file.display()
    );
    println!(
        "You can now use this wallet with the address: {}",
        address.to_string().green()
    );

    Ok(())
}

/// Update the wallets.json registry
fn update_wallets_registry(
    wallet_name: &str,
    address: &str,
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
        address: address.to_string(),
        network: network.to_string(),
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

    wallets.insert(wallet_name.to_string(), wallet_entry);
    fs::write(&wallets_file, serde_json::to_string_pretty(&wallets)?)?;

    Ok(())
}

/// Load generic wallet data from file
pub(crate) fn load_wallet_data(wallet_name: &str) -> Result<GenericWalletData, BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_name));

    if !wallet_file.exists() {
        return Err(BridgeCliError::WalletNotFound(wallet_name.to_string()));
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
        // Return empty HashMap if registry doesn't exist yet
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

/// Remove a wallet from the registry (wallets.json)
pub(crate) fn remove_wallet_from_registry(wallet_name: &str) -> Result<bool, BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    let wallets_file = storage_dir.join("wallets.json");

    if !wallets_file.exists() {
        // Registry doesn't exist, so wallet wasn't registered
        return Ok(false);
    }

    let wallets_content = fs::read_to_string(&wallets_file)
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Failed to read wallets registry: {}", e)))?;

    let mut wallets: HashMap<String, WalletRegistryEntry> = serde_json::from_str(&wallets_content)
        .map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!("Failed to parse wallets registry JSON: {}", e))
        })?;

    let was_removed = wallets.remove(wallet_name).is_some();

    if was_removed {
        fs::write(&wallets_file, serde_json::to_string_pretty(&wallets)?).map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!("Failed to write wallets registry: {}", e))
        })?;
    }

    Ok(was_removed)
}

/// Scan the storage directory for wallet files and extract their addresses
pub(crate) fn scan_wallet_files() -> Result<HashSet<String>, BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    let mut file_wallets: HashSet<String> = HashSet::new();

    if !storage_dir.exists() {
        return Ok(file_wallets);
    }

    for entry in fs::read_dir(&storage_dir)
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Failed to read storage directory: {}", e)))?
    {
        let entry = entry.map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!("Failed to read directory entry: {}", e))
        })?;

        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        // Check if it's a wallet file (wallet_*.json but not wallets.json)
        if file_name_str.starts_with("wallet_")
            && file_name_str.ends_with(".json")
            && file_name_str != "wallets.json"
        {
            let wallet_file_path = entry.path();
            match fs::read_to_string(&wallet_file_path) {
                Ok(wallet_content) => {
                    match serde_json::from_str::<GenericWalletData>(&wallet_content) {
                        Ok(wallet_data) => {
                            file_wallets.insert(wallet_data.wallet_name);
                        }
                        Err(e) => {
                            eprintln!(
                                "Warning: Failed to parse wallet file {}: {}",
                                file_name_str, e
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to read wallet file {}: {}",
                        file_name_str, e
                    );
                }
            }
        }
    }

    Ok(file_wallets)
}
