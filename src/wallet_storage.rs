use bitcoin::Network;
use serde::{Deserialize, Serialize};
use std::fs;
use std::{collections::HashMap, path::PathBuf};

use crate::encryption::{EncryptedData, EncryptedDataHex, encrypted_data_to_hex};
use crate::errors::BridgeCliError;

/// Generic wallet data structure that can handle different storage formats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericWalletData {
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
pub fn store_wallet_data(
    address: &str,
    network: Network,
    encrypted_mnemonic: &EncryptedData,
    encrypted_private_key: &EncryptedData,
    data_format: &str,
    imported: bool,
    import_method: Option<&str>,
) -> Result<(), BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    fs::create_dir_all(&storage_dir)?;

    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    if wallet_file.exists() {
        return Err(BridgeCliError::WalletAlreadyExists(address.to_string()));
    }

    let wallet_data = GenericWalletData {
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
    update_wallets_registry(address, network, imported, import_method)?;

    Ok(())
}

/// Update the wallets.json registry
pub fn update_wallets_registry(
    address: &str,
    network: Network,
    imported: bool,
    import_method: Option<&str>,
) -> Result<(), BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    let wallets_file = storage_dir.join("wallets.json");

    let mut wallets: HashMap<String, serde_json::Value> = if wallets_file.exists() {
        serde_json::from_str(&fs::read_to_string(&wallets_file)?)?
    } else {
        HashMap::new()
    };

    let mut wallet_entry = serde_json::json!({
        "network": network.to_string(),
        "created_at": chrono::Utc::now().to_rfc3339(),
        "secure": true,
    });

    if imported {
        wallet_entry["imported"] = serde_json::json!(true);
        wallet_entry["imported_at"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
        if let Some(method) = import_method {
            wallet_entry["import_method"] = serde_json::json!(method);
        }
    }

    wallets.insert(address.to_string(), wallet_entry);
    fs::write(&wallets_file, serde_json::to_string_pretty(&wallets)?)?;

    Ok(())
}

/// Load generic wallet data from file
pub fn load_wallet_data(address: &str) -> Result<GenericWalletData, BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    if !wallet_file.exists() {
        return Err(BridgeCliError::WalletNotFound(address.to_string()));
    }

    let json_data = fs::read_to_string(&wallet_file)?;
    let wallet_data: GenericWalletData = serde_json::from_str(&json_data)?;

    Ok(wallet_data)
}

/// Get the storage directory path
pub fn get_storage_dir() -> Result<PathBuf, BridgeCliError> {
    let home_dir = dirs::home_dir().ok_or(BridgeCliError::HomeDirectoryNotFound)?;
    Ok(home_dir.join(".clementine").join("keys"))
}
