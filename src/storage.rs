// Key storage functionality for Clementine CLI

use crate::BitcoinAddress;
use bitcoin::Network;
use bitcoin::secp256k1::Keypair;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use thiserror::Error;

/// Errors generated while storing/restoring secrets.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Unsupported network")]
    SerializationError(#[from] serde_json::Error),
    #[error("Error while interacting with input/output stream: {0}")]
    InputOutputError(#[from] std::io::Error),
    #[error("{0}")]
    BitcoinSecp256k1Error(#[from] bitcoin::secp256k1::Error),
    #[error("{0}")]
    BitcoinParseError(#[from] bitcoin::address::ParseError),

    #[error(transparent)]
    Eyre(#[from] eyre::Report),
}

/// Store a keypair and its corresponding taproot address
pub fn store_key(
    keypair: &Keypair,
    network: Network,
    passphrase: Option<&str>,
) -> Result<BitcoinAddress, StorageError> {
    // Check if passphrase encryption is requested
    if passphrase.is_some() {
        return Err(eyre::eyre!("Passphrase encryption is not yet implemented").into());
    }

    // Calculate the taproot address for this keypair
    let address = crate::bitcoin_utils::calculate_taproot_address(keypair, network);

    // Create storage directory
    let storage_dir = get_storage_dir()?;
    fs::create_dir_all(&storage_dir)?;

    // Store the keypair in plaintext (see #12)
    let key_file = storage_dir.join(format!("key_{}.json", address));
    let key_data = serde_json::json!({
        "network": network.to_string(),
        "address": address.to_string(),
        "private_key": keypair.secret_key().display_secret().to_string(),
        "public_key": keypair.public_key().to_string(),
        "stored_at": chrono::Utc::now().to_rfc3339()
    });
    fs::write(&key_file, serde_json::to_string_pretty(&key_data)?)?;

    // Set file permissions to 700 (rwx------)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&key_file)?.permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&key_file, perms)?;
    }

    // Store address in plaintext for easy lookup
    let address_file = storage_dir.join("addresses.json");
    let mut addresses: HashMap<String, serde_json::Value> = if address_file.exists() {
        serde_json::from_str(&fs::read_to_string(&address_file)?)?
    } else {
        HashMap::new()
    };

    addresses.insert(
        address.to_string(),
        serde_json::json!({
            "network": network.to_string(),
            "stored_at": chrono::Utc::now().to_rfc3339()
        }),
    );

    fs::write(address_file, serde_json::to_string_pretty(&addresses)?)?;

    tracing::info!("Key stored successfully");
    tracing::debug!("Address: {}", address);
    tracing::debug!("Network: {}", network);
    tracing::warn!("Key stored in plaintext (no encryption)",);

    Ok(address)
}

/// Load a keypair by its taproot address
pub fn load_key(
    taproot_address: &str,
    network: Network,
    passphrase: Option<&str>,
) -> Result<Keypair, StorageError> {
    // Check if passphrase encryption is requested
    if passphrase.is_some() {
        return Err(eyre::eyre!("Passphrase encryption is not yet implemented").into());
    }

    // Parse the address to validate it
    let unchecked_address: BitcoinAddress<bitcoin::address::NetworkUnchecked> =
        taproot_address.parse()?;
    let address = unchecked_address.assume_checked();

    // Load the keypair from storage
    let storage_dir = get_storage_dir()?;
    let key_file = storage_dir.join(format!("key_{}.json", address));

    if !key_file.exists() {
        return Err(eyre::eyre!("No key found for address: {}", address).into());
    }

    let key_data: serde_json::Value = serde_json::from_str(&fs::read_to_string(key_file)?)?;
    let private_key_str = key_data["private_key"]
        .as_str()
        .ok_or(eyre::eyre!("Invalid key file format: missing private_key"))?;

    // Parse the private key
    let secret_key = bitcoin::secp256k1::SecretKey::from_str(private_key_str)?;
    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &secret_key);

    tracing::info!("Key loaded successfully");
    tracing::debug!("Address: {}", address);
    tracing::debug!("Network: {}", network);
    tracing::warn!("Key loaded from plaintext storage");

    Ok(keypair)
}

/// Get the storage directory path
fn get_storage_dir() -> Result<PathBuf, StorageError> {
    let home_dir = dirs::home_dir().ok_or(eyre::eyre!("Could not determine home directory"))?;
    Ok(home_dir.join(".clementine").join("keys"))
}
