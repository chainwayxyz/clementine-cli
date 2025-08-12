// Key storage functionality for Clementine CLI

use bitcoin::secp256k1::Keypair;
use bitcoin::{Address, Network};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

/// Store a keypair and its corresponding taproot address
pub fn store_key(
    keypair: &Keypair,
    network: Network,
    passphrase: Option<&str>,
) -> Result<Address, Box<dyn std::error::Error>> {
    // Check if passphrase encryption is requested
    if passphrase.is_some() {
        return Err("Passphrase encryption is not yet implemented".into());
    }

    // Calculate the taproot address for this keypair
    let address = crate::bitcoin_utils::calculate_taproot_address(keypair, network);

    // Create storage directory
    let storage_dir = get_storage_dir()?;
    fs::create_dir_all(&storage_dir)?;

    // Store the keypair in plaintext (TODO: implement encryption)
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

    debug!("{} Key stored successfully", "SUCCESS".green().bold());
    debug!("{} {}", "ADDRESS".cyan().bold(), address);
    debug!("{} {}", "NETWORK".blue().bold(), network);
    debug!(
        "{} Key stored in plaintext (no encryption)",
        "WARNING".yellow().bold()
    );

    Ok(address)
}

/// Load a keypair by its taproot address
pub fn load_key(
    taproot_address: &str,
    network: Network,
    passphrase: Option<&str>,
) -> Result<Keypair, Box<dyn std::error::Error>> {
    // Check if passphrase encryption is requested
    if passphrase.is_some() {
        return Err("Passphrase encryption is not yet implemented".into());
    }

    // Parse the address to validate it
    let unchecked_address: Address<bitcoin::address::NetworkUnchecked> = taproot_address.parse()?;
    let address = unchecked_address.assume_checked();

    // Load the keypair from storage
    let storage_dir = get_storage_dir()?;
    let key_file = storage_dir.join(format!("key_{}.json", address));

    if !key_file.exists() {
        return Err(format!("No key found for address: {}", address).into());
    }

    let key_data: serde_json::Value = serde_json::from_str(&fs::read_to_string(key_file)?)?;
    let private_key_str = key_data["private_key"]
        .as_str()
        .ok_or("Invalid key file format: missing private_key")?;

    // Parse the private key
    let secret_key = bitcoin::secp256k1::SecretKey::from_str(private_key_str)?;
    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &secret_key);

    debug!("{} Key loaded successfully", "SUCCESS".green().bold());
    debug!("{} {}", "ADDRESS".cyan().bold(), address);
    debug!("{} {}", "NETWORK".blue().bold(), network);
    debug!(
        "{} Key loaded from plaintext storage",
        "WARNING".yellow().bold()
    );

    Ok(keypair)
}

/// Export the private key for a given taproot address
pub fn export_private_key(
    taproot_address: &str,
    network: Network,
) -> Result<String, Box<dyn std::error::Error>> {
    let unchecked_address: Address<bitcoin::address::NetworkUnchecked> = taproot_address.parse()?;
    let address = unchecked_address.require_network(network)?;

    let storage_dir = get_storage_dir()?;
    let key_file = storage_dir.join(format!("key_{}.json", address));

    if !key_file.exists() {
        return Err(format!("No key found for address: {}", address).into());
    }

    let key_data: serde_json::Value = serde_json::from_str(&fs::read_to_string(key_file)?)?;
    let private_key_str = key_data["private_key"]
        .as_str()
        .ok_or("Invalid key file format: missing private_key")?;

    Ok(private_key_str.to_string())
}

/// List all stored keys with their addresses and metadata
pub fn list_keys() -> Result<Vec<(String, serde_json::Value)>, Box<dyn std::error::Error>> {
    let storage_dir = get_storage_dir()?;
    let address_file = storage_dir.join("addresses.json");

    if !address_file.exists() {
        return Ok(Vec::new());
    }

    let addresses: HashMap<String, serde_json::Value> = 
        serde_json::from_str(&fs::read_to_string(&address_file)?)?;

    Ok(addresses.into_iter().collect())
}

/// Get the storage directory path
fn get_storage_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home_dir = dirs::home_dir().ok_or("Could not determine home directory")?;
    Ok(home_dir.join(".clementine").join("keys"))
}
