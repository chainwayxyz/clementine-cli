use anyhow::anyhow;
use bitcoin::key::Keypair;
use bitcoin::{Address, Network};
use colored::Colorize;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::str::FromStr;

use crate::BitcoinAddress;
use crate::encryption::{decrypt_private_key, encrypt_private_key};
use crate::mnemonic::{
    derive_private_key_from_mnemonic_secure, load_mnemonic_secure, load_private_key_secure,
    prompt_secure_passphrase,
};
use crate::private_key::derive_private_key;
use crate::secure_structs::SecureString;

/// Encrypted private key storage format
#[derive(Serialize, Deserialize)]
pub struct EncryptedKeyData {
    pub version: u8,
    pub encrypted: bool,
    pub network: String,
    pub address: String,
    pub crypto: CryptoParams,
    pub stored_at: String,
}

/// Cryptographic parameters for encrypted storage
#[derive(Serialize, Deserialize)]
pub struct CryptoParams {
    pub kdf: String,        // "argon2id"
    pub salt: String,       // hex encoded salt
    pub iterations: u32,    // argon2 time cost
    pub memory: u32,        // argon2 memory cost in KB
    pub parallelism: u32,   // argon2 parallelism
    pub cipher: String,     // "aes-256-gcm"
    pub nonce: String,      // hex encoded nonce
    pub ciphertext: String, // hex encoded encrypted private key
}

pub fn delete_wallet(address: &str) -> Result<(), anyhow::Error> {
    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));
    let wallets_file = storage_dir.join("wallets.json");

    // Check if wallet exists
    if !wallet_file.exists() {
        return Err(anyhow!("❌ No wallet found with address: {}", address));
    }

    // Load wallet data to verify it exists
    let _wallet_data: serde_json::Value = serde_json::from_str(&fs::read_to_string(&wallet_file)?)?;

    println!("{}", "🗑️  Wallet Deletion".red().bold());
    println!(
        "You are about to delete the wallet with address: {}",
        address.yellow()
    );
    println!("{}", "⚠️  This action cannot be undone!".red().bold());
    println!("Make sure you have backed up your mnemonic phrase before proceeding.");
    println!();

    // Confirm deletion
    print!("Type 'DELETE' to confirm deletion: ");
    io::stdout().flush()?;
    let mut confirmation = String::new();
    io::stdin().read_line(&mut confirmation)?;

    if confirmation.trim() != "DELETE" {
        println!("❌ Deletion cancelled.");
        return Ok(());
    }

    // Prompt for passphrase to verify access
    let passphrase = prompt_secure_passphrase("Enter passphrase to verify wallet access: ")?;

    // Try to decrypt both mnemonic and private key to verify passphrase is correct
    let mnemonic = load_mnemonic_secure(address, &passphrase)?;
    let stored_private_key = load_private_key_secure(address, &passphrase)?;

    // Derive private key from mnemonic
    let derived_private_key = derive_private_key_from_mnemonic_secure(&mnemonic)?;

    // Compare stored private key with derived private key
    if stored_private_key.expose_secret() != derived_private_key.expose_secret() {
        return Err(anyhow!(
            "❌ Wallet integrity check failed! The mnemonic and private key do not match.\n\
            This could indicate wallet corruption or tampering."
        ));
    }

    // If we got here, passphrase is correct and integrity check passed
    println!("🔒 Passphrase verified successfully");
    println!("✅ Wallet integrity check passed");
    println!("🗑️  Deleting wallet...");

    // Delete wallet file
    fs::remove_file(&wallet_file)?;
    println!("✅ Wallet file deleted: {}", wallet_file.display());

    // Remove from wallets.json registry
    if wallets_file.exists() {
        let mut wallets: HashMap<String, serde_json::Value> = {
            let wallets_content = fs::read_to_string(&wallets_file)?;
            serde_json::from_str(&wallets_content)?
        };

        if wallets.remove(address).is_some() {
            fs::write(&wallets_file, serde_json::to_string_pretty(&wallets)?)?;
            println!("✅ Wallet removed from registry");
        } else {
            println!("⚠️  Wallet was not found in registry");
        }
    }

    println!("{}", "✅ Wallet deleted successfully!".green().bold());
    println!(
        "{}",
        "⚠️  Remember: Your mnemonic phrase is the only way to recover this wallet.".yellow()
    );

    Ok(())
}

pub fn verify_wallet_integrity() -> Result<(), anyhow::Error> {
    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    let wallets_file = storage_dir.join("wallets.json");

    println!("{}", "🔍 Verifying Wallet Integrity".blue().bold());
    println!("Storage directory: {}", storage_dir.display());
    println!();

    // Read wallets.json registry
    let registry_wallets: HashSet<String> = if wallets_file.exists() {
        let wallets_content = fs::read_to_string(&wallets_file)?;
        let wallets: HashMap<String, serde_json::Value> = serde_json::from_str(&wallets_content)
            .map_err(|e| anyhow!("Failed to parse wallets.json: {}", e))?;
        wallets.keys().cloned().collect()
    } else {
        println!("⚠️  wallets.json not found - no registered wallets");
        HashSet::new()
    };

    // Scan for actual wallet files in storage directory
    let mut file_wallets: HashSet<String> = HashSet::new();

    if storage_dir.exists() {
        for entry in fs::read_dir(&storage_dir)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            // Check if it's a wallet file (wallet_ADDRESS.json)
            if file_name_str.starts_with("wallet_")
                && file_name_str.ends_with(".json")
                && file_name_str != "wallets.json"
            {
                // Extract address from filename
                let address = file_name_str
                    .strip_prefix("wallet_")
                    .and_then(|s| s.strip_suffix(".json"))
                    .unwrap_or("")
                    .to_string();

                if !address.is_empty() {
                    file_wallets.insert(address);
                }
            }
        }
    } else {
        println!("⚠️  Storage directory does not exist");
    }

    // Compare registry vs files
    let registry_only: HashSet<_> = registry_wallets.difference(&file_wallets).collect();
    let files_only: HashSet<_> = file_wallets.difference(&registry_wallets).collect();
    let matching: HashSet<_> = registry_wallets.intersection(&file_wallets).collect();

    // Report results
    println!("📊 Integrity Verification Results:");
    println!("  Total registered wallets: {}", registry_wallets.len());
    println!("  Total wallet files found: {}", file_wallets.len());
    println!("  Matching entries: {}", matching.len());
    println!();

    let mut has_issues = false;

    // Report wallets in registry but missing files
    if !registry_only.is_empty() {
        has_issues = true;
        println!("{} Wallets in registry but missing files:", "❌".red());
        for address in &registry_only {
            println!(
                "  • {} (file: wallet_{}.json not found)",
                address.yellow(),
                address
            );
        }
        println!();
    }

    // Report wallet files not in registry
    if !files_only.is_empty() {
        has_issues = true;
        println!("{} Wallet files not in registry:", "⚠️".yellow());
        for address in &files_only {
            println!(
                "  • {} (wallet_{}.json exists but not registered)",
                address.yellow(),
                address
            );
        }
        println!();
    }

    // Report successful matches
    if !matching.is_empty() {
        println!("{} Properly registered wallets:", "✅".green());
        for address in &matching {
            println!("  • {}", address.green());
        }
        println!();
    }

    // Summary
    if has_issues {
        println!("{}", "❌ Integrity issues found!".red().bold());
        println!("Consider:");
        if !registry_only.is_empty() {
            println!("• Remove orphaned registry entries or restore missing wallet files");
        }
        if !files_only.is_empty() {
            println!("• Register untracked wallet files or remove them if not needed");
        }
    } else if registry_wallets.is_empty() && file_wallets.is_empty() {
        println!(
            "{}",
            "ℹ️  No wallets found (this is normal for new installations)".blue()
        );
    } else {
        println!(
            "{}",
            "✅ All wallets are properly registered and files exist!"
                .green()
                .bold()
        );
    }

    Ok(())
}

/// Import a wallet from a file path
pub fn import_wallet_from_file(
    file_path: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let source_path = std::path::Path::new(file_path);

    if !source_path.exists() {
        return Err(format!("Wallet file does not exist: {}", file_path).into());
    }

    if !source_path.is_file() {
        return Err(format!("Path is not a file: {}", file_path).into());
    }

    // Read and validate the wallet file
    let wallet_content = fs::read_to_string(source_path)?;
    let wallet_data: serde_json::Value = serde_json::from_str(&wallet_content)?;

    // Extract wallet address from the file content
    let wallet_address = wallet_data["address"]
        .as_str()
        .ok_or("Invalid wallet file: missing address field")?;

    // Validate required fields
    if wallet_data["network"].is_null() {
        return Err("Invalid wallet file: missing network field".into());
    }

    // Check if encrypted data exists (either old format or new format)
    let has_encrypted_data =
        wallet_data["encrypted_data"].is_string() || wallet_data["encrypted_mnemonic"].is_object();

    if !has_encrypted_data {
        return Err("Invalid wallet file: missing encrypted data".into());
    }

    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    fs::create_dir_all(&storage_dir)?;

    let dest_wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_address));

    // Check if wallet already exists
    if dest_wallet_file.exists() {
        return Err(format!(
            "Wallet with address '{}' already exists in local storage",
            wallet_address
        )
        .into());
    }

    fs::copy(source_path, &dest_wallet_file)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dest_wallet_file)?.permissions();
        perms.set_mode(0o600); // rw-------
        fs::set_permissions(&dest_wallet_file, perms)?;
    }

    let wallets_file = storage_dir.join("wallets.json");
    let mut wallets: HashMap<String, serde_json::Value> = if wallets_file.exists() {
        serde_json::from_str(&fs::read_to_string(&wallets_file)?)?
    } else {
        HashMap::new()
    };

    let network = wallet_data["network"].as_str().unwrap_or("unknown");
    let default_created_at = chrono::Utc::now().to_rfc3339();
    let created_at = wallet_data["created_at"]
        .as_str()
        .unwrap_or(&default_created_at);
    let data_format = wallet_data["data_format"].as_str().unwrap_or("legacy");

    wallets.insert(
        wallet_address.to_string(),
        serde_json::json!({
            "network": network,
            "created_at": created_at,
            "imported_at": chrono::Utc::now().to_rfc3339(),
            "secure": true,
            "data_format": data_format,
            "imported": true
        }),
    );

    fs::write(wallets_file, serde_json::to_string_pretty(&wallets)?)?;

    println!(
        "{} Wallet '{}' imported successfully from: {}",
        "✓".green(),
        wallet_address.cyan(),
        file_path.yellow()
    );

    Ok(wallet_address.to_string())
}

/// List all stored keys with their addresses and metadata
pub fn list_keys() -> Result<Vec<(String, serde_json::Value)>, anyhow::Error> {
    let storage_dir = get_storage_dir()?;
    let address_file = storage_dir.join("addresses.json");

    if !address_file.exists() {
        return Ok(Vec::new());
    }

    let addresses: HashMap<String, serde_json::Value> =
        serde_json::from_str(&fs::read_to_string(&address_file)?)?;

    Ok(addresses.into_iter().collect())
}

pub fn load_wallet_from_file(file: &str) -> Result<serde_json::Value, anyhow::Error> {
    if !std::path::Path::new(file).exists() {
        return Err(anyhow!("Wallet file not found: {}", file));
    }

    let file_content = std::fs::read_to_string(file)?;
    let wallet_data: serde_json::Value = serde_json::from_str(&file_content)?;

    Ok(wallet_data)
}

/// Store a keypair and its corresponding taproot address
pub fn store_key(
    keypair: &Keypair,
    network: Network,
    passphrase: SecureString,
) -> Result<BitcoinAddress, anyhow::Error> {
    // Calculate the taproot address for this keypair
    let address = crate::bitcoin_utils::calculate_taproot_address(keypair, network);

    // Create storage directory
    let storage_dir = get_storage_dir()?;
    fs::create_dir_all(&storage_dir)?;

    let key_file = storage_dir.join(format!("key_{address}.json"));
    let private_key_str = keypair.secret_key().display_secret().to_string();

    let crypto = encrypt_private_key(&private_key_str, &passphrase)?;

    let encrypted_data = EncryptedKeyData {
        version: 2,
        encrypted: true,
        network: network.to_string(),
        address: address.to_string(),
        crypto,
        stored_at: chrono::Utc::now().to_rfc3339(),
    };

    let key_data = serde_json::to_string_pretty(&encrypted_data)?;

    fs::write(&key_file, key_data)?;

    // Set file permissions to 700 (rwx------)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&key_file)?.permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&key_file, perms)?;
    }

    // Store address for easy lookup
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
        "{} Key stored with AES-256-GCM encryption",
        "ENCRYPTED".green().bold()
    );

    Ok(address)
}

/// Load a keypair by its taproot address
pub fn load_key(
    taproot_address: &str,
    network: Network,
    passphrase: Option<&SecureString>,
) -> Result<Keypair, anyhow::Error> {
    // Parse the address to validate it
    let unchecked_address: BitcoinAddress<bitcoin::address::NetworkUnchecked> =
        taproot_address.parse()?;
    let address = unchecked_address.assume_checked();

    // Load the keypair from storage
    let storage_dir = get_storage_dir()?;
    let key_file = storage_dir.join(format!("key_{address}.json"));

    if !key_file.exists() {
        return Err(anyhow!("No key found for address: {address}").into());
    }

    let file_content = fs::read_to_string(key_file)?;
    let key_data: serde_json::Value = serde_json::from_str(&file_content)?;

    // Detect key format version
    let version = key_data
        .get("version")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    let is_encrypted = key_data
        .get("encrypted")
        .and_then(|e| e.as_bool())
        .unwrap_or(false);

    let private_key_str = if is_encrypted && version >= 2 {
        // Handle encrypted key (version 2+)
        let passphrase = passphrase.ok_or(
            anyhow!("This key is encrypted and requires a passphrase. Please provide the passphrase used when the key was created.")
        )?;

        let encrypted_data: EncryptedKeyData = serde_json::from_str(&file_content)?;
        let decrypted = decrypt_private_key(&encrypted_data.crypto, &passphrase)?;
        decrypted.expose_secret().to_owned()
    } else {
        // Handle legacy format keys - these should be migrated
        return Err(anyhow!(
            "This key uses an old storage format. Please regenerate your key to use the current secure storage format."
        ));
    };

    // Parse the private key
    let secret_key = bitcoin::secp256k1::SecretKey::from_str(&private_key_str)?;
    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &secret_key);

    debug!("{} Key loaded successfully", "SUCCESS".green().bold());
    debug!("{} {}", "ADDRESS".cyan().bold(), address);
    debug!("{} {}", "NETWORK".blue().bold(), network);
    debug!(
        "{} Key loaded from encrypted storage",
        "DECRYPTED".green().bold()
    );

    Ok(keypair)
}

/// Export the private key for a given taproot address
pub fn export_private_key(
    taproot_address: &str,
    network: Network,
) -> Result<String, anyhow::Error> {
    let unchecked_address: Address<bitcoin::address::NetworkUnchecked> = taproot_address.parse()?;
    let address = unchecked_address.require_network(network)?;

    let storage_dir = get_storage_dir()?;
    let key_file = storage_dir.join(format!("key_{address}.json"));

    if !key_file.exists() {
        return Err(anyhow!("No key found for address: {address}"));
    }

    let key_data: serde_json::Value = serde_json::from_str(&fs::read_to_string(key_file)?)?;
    let private_key_str = key_data["private_key"]
        .as_str()
        .ok_or(anyhow!("Invalid key file format: missing private_key"))?;

    Ok(private_key_str.to_string())
}

/// Get the storage directory path
pub fn get_storage_dir() -> Result<PathBuf, anyhow::Error> {
    let home_dir = dirs::home_dir().ok_or(anyhow!("Could not determine home directory"))?;
    Ok(home_dir.join(".clementine").join("keys"))
}

pub fn derive_keypair_and_address(
    master_seed: &[u8; 32],
    derivation_path: &str,
    network: Network,
) -> Result<(Keypair, Address), anyhow::Error> {
    let secret_key = derive_private_key(master_seed, derivation_path, network)?;
    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &secret_key);
    let address = crate::bitcoin_utils::calculate_taproot_address(&keypair, network);

    Ok((keypair, address))
}

pub fn get_standard_derivation_path(account: u32, change: u32, address_index: u32) -> String {
    format!("m/44'/0'/{account}'/{change}/{address_index}")
}

pub fn get_native_segwit_derivation_path(account: u32, change: u32, address_index: u32) -> String {
    format!("m/84'/0'/{account}'/{change}/{address_index}")
}

pub fn get_taproot_derivation_path(account: u32, change: u32, address_index: u32) -> String {
    format!("m/86'/0'/{account}'/{change}/{address_index}")
}

#[cfg(test)]
pub mod tests {
    use crate::{passphrase::derive_key_from_passphrase, secure_structs::SecureByteSlice};

    use super::*;
    use bitcoin::secp256k1::{Keypair, Secp256k1, SecretKey};
    use std::fs;

    /// Test helper to store a key with custom base directory
    #[cfg(test)]
    pub fn store_key_with_base_dir(
        keypair: &Keypair,
        network: Network,
        passphrase: &str,
        base_dir: &std::path::Path,
    ) -> Result<BitcoinAddress, anyhow::Error> {
        use secrecy::SecretBox;

        let address = crate::bitcoin_utils::calculate_taproot_address(keypair, network);
        let storage_dir = base_dir.join(".clementine").join("keys");
        fs::create_dir_all(&storage_dir)?;

        println!("Storing key for address: {address}");

        let key_file = storage_dir.join(format!("key_{address}.json"));
        let private_key_str = keypair.secret_key().display_secret().to_string();

        println!("Private key for address {address}: {private_key_str}");

        let secure_passphrase = SecretBox::init_with(|| passphrase.to_string());
        let crypto = encrypt_private_key(&private_key_str, &secure_passphrase)?;

        println!("Secure passphrase is generated");

        let encrypted_data = EncryptedKeyData {
            version: 2,
            encrypted: true,
            network: network.to_string(),
            address: address.to_string(),
            crypto,
            stored_at: chrono::Utc::now().to_rfc3339(),
        };

        println!("Encrypted key data generated.");

        let key_data = serde_json::to_string_pretty(&encrypted_data)?;
        fs::write(&key_file, key_data)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&key_file)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&key_file, perms)?;
        }

        Ok(address)
    }

    /// Test helper to load a key with custom base directory
    #[cfg(test)]
    pub fn load_key_with_base_dir(
        address: &str,
        network: Network,
        passphrase: Option<&str>,
        base_dir: &std::path::Path,
    ) -> Result<Keypair, anyhow::Error> {
        use bitcoin::secp256k1::{Secp256k1, SecretKey};
        let storage_dir = base_dir.join(".clementine").join("keys");
        let key_file = storage_dir.join(format!("key_{address}.json"));

        if !key_file.exists() {
            return Err(anyhow!("Key file not found for address: {address}"));
        }

        let key_data = fs::read_to_string(&key_file)?;
        let encrypted_data: EncryptedKeyData = serde_json::from_str(&key_data)?;

        if encrypted_data.network != network.to_string() {
            return Err(anyhow!(
                "Key network mismatch: expected {}, found {}",
                network,
                encrypted_data.network
            ));
        }

        if encrypted_data.encrypted {
            match passphrase {
                Some(pass) => {
                    let secure_passphrase = SecureString::init_with(|| pass.to_string());
                    let decrypted_key =
                        decrypt_private_key(&encrypted_data.crypto, &secure_passphrase)?;
                    let secp = Secp256k1::new();
                    let secret_key = SecretKey::from_str(decrypted_key.expose_secret())?;
                    Ok(Keypair::from_secret_key(&secp, &secret_key))
                }
                None => Err(anyhow!("Key is encrypted and requires a passphrase")),
            }
        } else {
            Err(anyhow!(
                "Unencrypted keys are not supported in this version"
            ))
        }
    }

    #[test]
    fn test_derive_key_from_passphrase() {
        // Test basic key derivation
        let passphrase = SecureString::init_with(|| "test_passphrase_123".to_string());
        let salt = [1u8; 32];

        let key1 = derive_key_from_passphrase(&passphrase, &salt, 1000, 1024, 1).unwrap();
        let key2 = derive_key_from_passphrase(&passphrase, &salt, 1000, 1024, 1).unwrap();

        // Same inputs should produce same key
        assert_eq!(key1.expose_secret(), key2.expose_secret());
    }

    #[test]
    fn test_derive_key_different_inputs() {
        let passphrase1 = SecureString::init_with(|| "passphrase1".to_string());
        let passphrase2 = SecureString::init_with(|| "passphrase2".to_string());
        let salt = [1u8; 32];

        let key1 = derive_key_from_passphrase(&passphrase1, &salt, 1000, 1024, 1).unwrap();
        let key2 = derive_key_from_passphrase(&passphrase2, &salt, 1000, 1024, 1).unwrap();

        // Different passphrases should produce different keys
        assert_ne!(key1.expose_secret(), key2.expose_secret());
    }

    #[test]
    fn test_derive_key_different_salts() {
        let passphrase = SecureString::init_with(|| "same_passphrase".to_string());
        let salt1 = [1u8; 32];
        let salt2 = [2u8; 32];

        let key1 = derive_key_from_passphrase(&passphrase, &salt1, 1000, 1024, 1).unwrap();
        let key2 = derive_key_from_passphrase(&passphrase, &salt2, 1000, 1024, 1).unwrap();

        // Different salts should produce different keys
        assert_ne!(key1.expose_secret(), key2.expose_secret());
    }

    #[test]
    fn test_encrypt_decrypt_round_trip() {
        let private_key = "L1HKVVLHXiUhecWnwFYF6L3shkf1E12HUmuZTESvBXUdx3yqVP1D";
        let passphrase = SecureString::init_with(|| "strong_passphrase_123".to_string());

        // Encrypt the private key
        let crypto_params = encrypt_private_key(private_key, &passphrase).unwrap();

        // Verify crypto parameters are properly set
        assert_eq!(crypto_params.kdf, "argon2id");
        assert_eq!(crypto_params.cipher, "aes-256-gcm");
        assert_eq!(crypto_params.iterations, 3);
        assert_eq!(crypto_params.memory, 65_536);
        assert_eq!(crypto_params.parallelism, 4);
        assert!(!crypto_params.salt.is_empty());
        assert!(!crypto_params.nonce.is_empty());
        assert!(!crypto_params.ciphertext.is_empty());

        // Decrypt the private key
        let decrypted = decrypt_private_key(&crypto_params, &passphrase).unwrap();

        // Should match original
        assert_eq!(decrypted.expose_secret(), private_key);
    }

    #[test]
    fn test_decrypt_wrong_passphrase() {
        let private_key = "L1HKVVLHXiUhecWnwFYF6L3shkf1E12HUmuZTESvBXUdx3yqVP1D";
        let correct_passphrase = SecureString::init_with(|| "correct_passphrase".to_string());
        let wrong_passphrase = SecureString::init_with(|| "wrong_passphrase".to_string());

        // Encrypt with correct passphrase
        let crypto_params = encrypt_private_key(private_key, &correct_passphrase).unwrap();

        // Try to decrypt with wrong passphrase - should fail
        let result = decrypt_private_key(&crypto_params, &wrong_passphrase);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Decryption failed")
        );
    }

    #[test]
    fn test_encrypt_different_passphrases_different_output() {
        let private_key = "L1HKVVLHXiUhecWnwFYF6L3shkf1E12HUmuZTESvBXUdx3yqVP1D";
        let passphrase1 = SecureString::init_with(|| "passphrase1".to_string());
        let passphrase2 = SecureString::init_with(|| "passphrase2".to_string());

        let crypto1 = encrypt_private_key(private_key, &passphrase1).unwrap();
        let crypto2 = encrypt_private_key(private_key, &passphrase2).unwrap();

        // Different passphrases should produce different encrypted results
        assert_ne!(crypto1.ciphertext, crypto2.ciphertext);
        assert_ne!(crypto1.salt, crypto2.salt);
        assert_ne!(crypto1.nonce, crypto2.nonce);
    }

    #[test]
    fn test_encrypt_same_passphrase_different_output() {
        let private_key = "L1HKVVLHXiUhecWnwFYF6L3shkf1E12HUmuZTESvBXUdx3yqVP1D";
        let passphrase = SecureString::init_with(|| "same_passphrase".to_string());

        let crypto1 = encrypt_private_key(private_key, &passphrase).unwrap();
        let crypto2 = encrypt_private_key(private_key, &passphrase).unwrap();

        // Same passphrase should still produce different encrypted results due to random salt/nonce
        assert_ne!(crypto1.ciphertext, crypto2.ciphertext);
        assert_ne!(crypto1.salt, crypto2.salt);
        assert_ne!(crypto1.nonce, crypto2.nonce);
    }

    #[test]
    fn test_decrypt_unsupported_kdf() {
        let passphrase = SecureString::init_with(|| "test_passphrase".to_string());
        let crypto = CryptoParams {
            kdf: "pbkdf2".to_string(), // Unsupported KDF
            salt: hex::encode([1u8; 32]),
            iterations: 3,
            memory: 1024,
            parallelism: 1,
            cipher: "aes-256-gcm".to_string(),
            nonce: hex::encode([2u8; 12]),
            ciphertext: hex::encode([3u8; 32]),
        };

        let result = decrypt_private_key(&crypto, &passphrase);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported KDF"));
    }

    #[test]
    fn test_decrypt_unsupported_cipher() {
        let passphrase = SecureString::init_with(|| "test_passphrase".to_string());
        let crypto = CryptoParams {
            kdf: "argon2id".to_string(),
            salt: hex::encode([1u8; 32]),
            iterations: 3,
            memory: 1024,
            parallelism: 1,
            cipher: "aes-128-cbc".to_string(), // Unsupported cipher
            nonce: hex::encode([2u8; 12]),
            ciphertext: hex::encode([3u8; 32]),
        };

        let result = decrypt_private_key(&crypto, &passphrase);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unsupported cipher")
        );
    }

    #[test]
    fn test_decrypt_invalid_hex() {
        let passphrase = SecureString::init_with(|| "test_passphrase".to_string());
        let crypto = CryptoParams {
            kdf: "argon2id".to_string(),
            salt: "invalid_hex_xyz".to_string(), // Invalid hex
            iterations: 3,
            memory: 1024,
            parallelism: 1,
            cipher: "aes-256-gcm".to_string(),
            nonce: hex::encode([2u8; 12]),
            ciphertext: hex::encode([3u8; 32]),
        };

        let result = decrypt_private_key(&crypto, &passphrase);
        assert!(result.is_err());
    }

    #[test]
    fn test_secure_string_zeroize() {
        let secret_data = "sensitive_private_key_data";
        let secure_string = SecureString::init_with(|| secret_data.to_string());

        assert_eq!(secure_string.expose_secret(), secret_data);
        assert_eq!(
            secure_string.expose_secret().as_bytes(),
            secret_data.as_bytes()
        );

        // SecureString should implement ZeroizeOnDrop
        // We can't directly test the zeroing behavior in a unit test,
        // but we can verify the type implements the trait
        drop(secure_string);
    }

    #[test]
    fn test_derived_key_zeroize() {
        let key_bytes = [42u8; 32];
        let derived_key = SecureByteSlice::init_with(|| key_bytes);

        assert_eq!(derived_key.expose_secret(), &key_bytes);

        // DerivedKey should implement ZeroizeOnDrop
        drop(derived_key);
    }

    #[test]
    fn test_encrypted_key_data_serialization() {
        let crypto = CryptoParams {
            kdf: "argon2id".to_string(),
            salt: hex::encode([1u8; 32]),
            iterations: 3,
            memory: 65_536,
            parallelism: 4,
            cipher: "aes-256-gcm".to_string(),
            nonce: hex::encode([2u8; 12]),
            ciphertext: hex::encode([3u8; 64]),
        };

        let encrypted_data = EncryptedKeyData {
            version: 2,
            encrypted: true,
            network: "bitcoin".to_string(),
            address: "bc1pdqrcrxa8vx6gy75mfdfj84puhxffh4fq46h3gkp6jxdd0vjcsdyspfxcv6".to_string(),
            crypto,
            stored_at: "2024-01-01T00:00:00Z".to_string(),
        };

        // Test serialization
        let json = serde_json::to_string(&encrypted_data).unwrap();
        assert!(json.contains("\"version\":2"));
        assert!(json.contains("\"encrypted\":true"));
        assert!(json.contains("\"kdf\":\"argon2id\""));
        assert!(json.contains("\"cipher\":\"aes-256-gcm\""));

        // Test deserialization
        let deserialized: EncryptedKeyData = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.version, 2);
        assert!(deserialized.encrypted);
        assert_eq!(deserialized.crypto.kdf, "argon2id");
        assert_eq!(deserialized.crypto.iterations, 3);
    }

    // Integration tests for store_key and load_key
    #[test]
    fn test_store_and_load_key_integration() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path();
        println!("Base directory for key storage: {}", temp_path.display());

        // Create a test keypair
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let network = Network::Testnet4;
        let passphrase = "test_passphrase_123";

        println!("Passphrase for key storage: {passphrase}");

        // Store the key using helper function
        let stored_address =
            store_key_with_base_dir(&keypair, network, passphrase, temp_path).unwrap();

        println!("Stored key address: {stored_address}");

        // Load the key back using helper function
        let loaded_keypair = load_key_with_base_dir(
            &stored_address.to_string(),
            network,
            Some(passphrase),
            temp_path,
        )
        .unwrap();

        println!("Loaded key address: {stored_address}");

        // Verify the loaded keypair matches the original
        assert_eq!(keypair.secret_key(), loaded_keypair.secret_key());
        assert_eq!(keypair.public_key(), loaded_keypair.public_key());

        println!("Key successfully stored and loaded for address: {stored_address}");

        // Verify file exists and has correct permissions
        let storage_dir = temp_path.join(".clementine").join("keys");
        let key_file = storage_dir.join(format!("key_{stored_address}.json"));
        assert!(key_file.exists());

        println!("Key file exists: {}", key_file.display());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&key_file).unwrap();
            let permissions = metadata.permissions().mode();
            // Check that permissions are 600 (rw-------)
            assert_eq!(permissions & 0o777, 0o600);
        }

        println!("Key file permissions are correct: 600 (rw-------)");

        // Temporary directory will be automatically cleaned up when temp_dir goes out of scope
    }

    #[test]
    fn test_load_key_wrong_passphrase() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_dir = temp_dir.path();

        // Create and store a key
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let network = Network::Testnet4;
        let correct_passphrase = "correct_passphrase";
        let wrong_passphrase = "wrong_passphrase";

        let stored_address =
            store_key_with_base_dir(&keypair, network, correct_passphrase, base_dir).unwrap();

        // Try to load with wrong passphrase - should fail
        let result = load_key_with_base_dir(
            &stored_address.to_string(),
            network,
            Some(wrong_passphrase),
            base_dir,
        );
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Decryption failed")
        );

        // Temporary directory will be automatically cleaned up when temp_dir goes out of scope
    }

    #[test]
    fn test_load_nonexistent_key() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_dir = temp_dir.path();

        let network = Network::Testnet4;
        let fake_address = "tb1pdqrcrxa8vx6gy75mfdfj84puhxffh4fq46h3gkp6jxdd0vjcsdysn6k0k7";

        let result =
            load_key_with_base_dir(fake_address, network, Some("any_passphrase"), base_dir);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Key file not found for address")
        );

        // Temporary directory will be automatically cleaned up when temp_dir goes out of scope
    }

    #[test]
    fn test_multiple_keys_storage() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_dir = temp_dir.path();

        let secp = Secp256k1::new();
        let network = Network::Testnet4;

        // Store multiple keys
        let mut stored_addresses = Vec::new();
        for i in 1..=3 {
            let secret_key = SecretKey::from_slice(&[i; 32]).unwrap();
            let keypair = Keypair::from_secret_key(&secp, &secret_key);
            let passphrase = format!("passphrase_{i}");

            let address =
                store_key_with_base_dir(&keypair, network, &passphrase, base_dir).unwrap();
            stored_addresses.push((address, passphrase, keypair));
        }

        // Load each key and verify
        for (address, passphrase, original_keypair) in stored_addresses {
            let loaded_keypair =
                load_key_with_base_dir(&address.to_string(), network, Some(&passphrase), base_dir)
                    .unwrap();
            assert_eq!(original_keypair.secret_key(), loaded_keypair.secret_key());
        }

        // Temporary directory will be automatically cleaned up when temp_dir goes out of scope
    }

    #[test]
    fn test_key_derivation_parameters() {
        let passphrase = SecureString::init_with(|| "test_passphrase".to_string());
        let salt = [1u8; 32];

        // Test with minimum secure parameters
        let key_min = derive_key_from_passphrase(&passphrase, &salt, 3, 1024, 1).unwrap();

        // Test with production parameters (same as used in encrypt_private_key)
        let key_prod = derive_key_from_passphrase(&passphrase, &salt, 3, 65_536, 4).unwrap();

        // Both should succeed but produce different keys due to different parameters
        assert_ne!(key_min.expose_secret(), key_prod.expose_secret());
    }

    #[test]
    fn test_argon2_invalid_parameters() {
        let passphrase = SecureString::init_with(|| "test_passphrase".to_string());
        let salt = [1u8; 32];

        // Test with invalid memory parameter (too small)
        let result = derive_key_from_passphrase(&passphrase, &salt, 1, 0, 1);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid Argon2 parameters")
        );
    }
}
