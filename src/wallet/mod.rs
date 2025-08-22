pub(crate) mod address;
mod encryption;
mod mnemonic;
pub(crate) mod passphrase;
mod wallet_storage;
pub(crate) mod wallet_utils;

pub use address::get_all_wallets_with_addresses;
pub use mnemonic::show_mnemonic_secure;

use bitcoin::Network;
use colored::Colorize;
use eyre::eyre;
use secrecy::ExposeSecret;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use zeroize::Zeroize;

use crate::bitcoin_utils::{SECP, calculate_taproot_address};
use bitcoin::secp256k1::{Keypair, SecretKey};

use crate::errors::BridgeCliError;
use crate::secure_display::{display_mnemonic_securely, display_private_key_securely};
use crate::structs::SecureString;
use encryption::{aes_decrypt_secure, aes_encrypt_secure};
use mnemonic::{
    MNEMONIC_WORD_COUNT, derive_private_key_from_mnemonic_secure, generate_mnemonic_secure,
    prompt_mnemonic_secure,
};
use passphrase::{prompt_passphrase, prompt_unlock_passphrase};
use wallet_storage::get_storage_dir;
use wallet_utils::{
    load_key_and_address, parse_network, validate_mnemonic_import, validate_private_key_import,
    validate_wallet_availability, wallet_exists,
};

pub fn create_encrypted_wallet_with_address(
    network: Network,
    name: String,
) -> Result<SecureString, BridgeCliError> {
    println!();

    // Generate mnemonic
    let secure_mnemonic = generate_mnemonic_secure()?;

    // Generate address from mnemonic using helper function
    let address = address::generate_address_from_mnemonic_secure(&secure_mnemonic, network)
        .map_err(|e| BridgeCliError::AddressGenerationFromMnemonicFailed(e.to_string()))?;

    // Validate that both wallet name and address don't already exist
    validate_wallet_availability(&name, &address.to_string(), network)?;

    // Prompt for passphrase
    let passphrase = prompt_passphrase(true)?;

    // Encrypt mnemonic and private key separately with different nonces
    let master_private_key_secure = derive_private_key_from_mnemonic_secure(&secure_mnemonic)?;

    let encrypted_mnemonic = aes_encrypt_secure(&secure_mnemonic, &passphrase)?;
    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)?;

    // Store encrypted wallet with separate encrypted fields
    wallet_storage::store_wallet_data(
        &address.to_string(),
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
        "separate_encrypted_fields",
        false,
        None,
        &name,
    )?;

    match display_mnemonic_securely(&secure_mnemonic) {
        Ok(()) => {
            println!("{}", "Mnemonic displayed securely".green());
        }
        Err(e) => {
            eprintln!(
                "{} Failed to display mnemonic securely: {}",
                "ERROR".red().bold(),
                e
            );
            eprintln!(
                "{} The wallet is still safely stored encrypted.",
                "INFO".blue().bold()
            );
        }
    }

    Ok(secure_mnemonic)
}

pub fn delete_wallet(wallet_name: &str) -> Result<(), BridgeCliError> {
    // Check if wallet exists
    if !wallet_exists(wallet_name)? {
        return Err(BridgeCliError::WalletNotFound(wallet_name.to_string()));
    }

    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_name));
    // Load wallet data to verify it exists and get the address
    let wallet_data: serde_json::Value = serde_json::from_str(&fs::read_to_string(&wallet_file)?)?;
    let address = wallet_data["address"]
        .as_str()
        .ok_or_else(|| eyre!("Invalid wallet file: missing address field"))?;

    println!("{}", "Wallet Deletion".red().bold());
    println!(
        "You are about to delete the wallet '{}' with address: {}",
        wallet_name.cyan(),
        address.yellow()
    );
    println!("{}", "This action cannot be undone!".red().bold());
    println!("Make sure you have backed up your mnemonic phrase before proceeding.");
    println!();

    // Confirm deletion
    print!("Type 'DELETE' to confirm deletion: ");
    io::stdout().flush()?;
    let mut confirmation = String::new();
    io::stdin().read_line(&mut confirmation)?;

    if confirmation.trim() != "DELETE" {
        println!("Deletion cancelled.");
        return Ok(());
    }

    println!("Deleting wallet...");

    let wallets_file = storage_dir.join("wallets.json");
    // Delete wallet file
    fs::remove_file(&wallet_file)?;
    println!("Wallet file deleted: {}", wallet_file.display());

    // Remove from wallets.json registry
    if wallets_file.exists() {
        let mut wallets: HashMap<String, serde_json::Value> = {
            let wallets_content = fs::read_to_string(&wallets_file)?;
            serde_json::from_str(&wallets_content)?
        };

        if wallets.remove(wallet_name).is_some() {
            fs::write(&wallets_file, serde_json::to_string_pretty(&wallets)?)?;
            println!("Wallet removed from registry");
        } else {
            println!("Wallet was not found in registry");
        }
    }

    println!("{}", "Wallet deleted successfully!".green().bold());

    Ok(())
}

/// Backup a wallet file to a specified destination
pub fn backup_wallet(wallet_name: &str, destination_path: &str) -> Result<(), BridgeCliError> {
    if !wallet_exists(wallet_name)? {
        return Err(BridgeCliError::WalletNotFound(wallet_name.to_string()));
    }

    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_name));

    if !wallet_file.exists() {
        return Err(BridgeCliError::WalletNotFound(wallet_name.to_string()));
    }

    // Parse the destination path
    let dest_path = std::path::Path::new(destination_path);

    // If destination is a directory, create the filename
    let final_dest = if dest_path.is_dir() {
        dest_path.join(format!("wallet_{}.json", wallet_name))
    } else {
        dest_path.to_path_buf()
    };

    // Create parent directories if they don't exist
    if let Some(parent) = final_dest.parent() {
        fs::create_dir_all(parent)?;
    }

    // Copy the wallet file
    fs::copy(&wallet_file, &final_dest)?;

    // Set secure file permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&final_dest)?.permissions();
        perms.set_mode(0o600); // rw-------
        fs::set_permissions(&final_dest, perms)?;
    }

    println!(
        "{} Wallet '{}' backed up successfully to: {}",
        "SUCCESS".green(),
        wallet_name.cyan(),
        final_dest.display().to_string().yellow()
    );

    Ok(())
}

/// Import a wallet using secure mnemonic input (step-by-step) and password creation
pub fn import_wallet_from_mnemonic(
    network: Network,
    wallet_name: &str,
) -> Result<String, BridgeCliError> {
    println!("{}", "Import Wallet with Mnemonic".blue().bold());

    // Prompt for mnemonic securely (word by word)
    println!("{}", "Step 1: Enter Mnemonic Phrase".yellow().bold());
    let secure_mnemonic = prompt_mnemonic_secure()?;

    // Generate address from mnemonic using helper function
    let address = address::generate_address_from_mnemonic_secure(&secure_mnemonic, network)
        .map_err(|e| BridgeCliError::AddressGenerationFromMnemonicFailed(e.to_string()))?;

    // Validate that both wallet name and address don't already exist
    validate_wallet_availability(wallet_name, &address.to_string(), network)?;

    println!();
    println!("Mnemonic processed successfully!");
    println!("Derived address: {}", address.to_string().green());
    println!();

    if wallet_exists(&address.to_string())? {
        return Err(BridgeCliError::WalletAlreadyExists(wallet_name.to_string()));
    }

    // Securely prompt for passphrase
    println!("{}", "Step 2: Create Secure Passphrase".yellow().bold());
    println!("Enter a strong passphrase to encrypt your imported wallet:");
    let passphrase = prompt_passphrase(false)?;

    println!("Passphrase created successfully!");
    println!();

    // Encrypt and store wallet
    println!(
        "{}",
        "Step 3: Encrypting and Storing Wallet".yellow().bold()
    );

    let master_private_key_secure = derive_private_key_from_mnemonic_secure(&secure_mnemonic)
        .map_err(|e| BridgeCliError::PrivateKeyDerivationFromMnemonicFailed(e.to_string()))?;

    let encrypted_mnemonic = aes_encrypt_secure(&secure_mnemonic, &passphrase)
        .map_err(|e| BridgeCliError::MnemonicEncryptionFailed(e.to_string()))?;

    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .map_err(|e| BridgeCliError::PrivateKeyEncryptionFailed(e.to_string()))?;

    wallet_storage::store_wallet_data(
        &address.to_string(),
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
        "separate_encrypted_fields",
        true,
        Some("mnemonic_import"),
        wallet_name,
    )
    .map_err(|e| BridgeCliError::WalletStorageFailed(e.to_string()))?;

    Ok(address.to_string())
}

pub fn verify_wallet_integrity() -> Result<(), BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    let wallets_file = storage_dir.join("wallets.json");

    println!("{}", "Verifying Wallet Integrity".blue().bold());
    println!("Storage directory: {}", storage_dir.display());
    println!();

    // Read wallets.json registry
    let registry_wallets: HashSet<String> = if wallets_file.exists() {
        let wallets_content = fs::read_to_string(&wallets_file)?;
        let wallets: HashMap<String, serde_json::Value> = serde_json::from_str(&wallets_content)
            .map_err(|e| BridgeCliError::WalletsJsonParseFailed(e.to_string()))?;
        wallets.keys().cloned().collect()
    } else {
        println!("wallets.json not found - no registered wallets");
        HashSet::new()
    };

    // Scan for actual wallet files in storage directory
    let mut file_wallets: HashSet<String> = HashSet::new();

    if storage_dir.exists() {
        for entry in fs::read_dir(&storage_dir)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            // Check if it's a wallet file (wallet_*.json)
            if file_name_str.starts_with("wallet_")
                && file_name_str.ends_with(".json")
                && file_name_str != "wallets.json"
            {
                // Read the wallet file and extract address from JSON content
                let wallet_file_path = entry.path();
                match fs::read_to_string(&wallet_file_path) {
                    Ok(wallet_content) => {
                        match serde_json::from_str::<serde_json::Value>(&wallet_content) {
                            Ok(wallet_data) => {
                                if let Some(address) = wallet_data["address"].as_str() {
                                    file_wallets.insert(address.to_string());
                                } else {
                                    println!(
                                        "Warning: Wallet file {} is missing address field",
                                        file_name_str.yellow()
                                    );
                                }
                            }
                            Err(e) => {
                                println!(
                                    "Warning: Failed to parse wallet file {}: {}",
                                    file_name_str.yellow(),
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!(
                            "Warning: Failed to read wallet file {}: {}",
                            file_name_str.yellow(),
                            e
                        );
                    }
                }
            }
        }
    } else {
        println!("Storage directory does not exist");
    }

    // Compare registry vs files
    let registry_only: HashSet<_> = registry_wallets.difference(&file_wallets).collect();
    let files_only: HashSet<_> = file_wallets.difference(&registry_wallets).collect();
    let matching: HashSet<_> = registry_wallets.intersection(&file_wallets).collect();

    // Report results
    println!("Integrity Verification Results:");
    println!("  Total registered wallets: {}", registry_wallets.len());
    println!("  Total wallet files found: {}", file_wallets.len());
    println!("  Matching entries: {}", matching.len());
    println!();

    let mut has_issues = false;

    // Report wallets in registry but missing files
    if !registry_only.is_empty() {
        has_issues = true;
        println!("Wallets in registry but missing files:");
        for address in &registry_only {
            println!(
                "  - {} (file: wallet_{}.json not found)",
                address.yellow(),
                address
            );
        }
        println!();
    }

    // Report wallet files not in registry
    if !files_only.is_empty() {
        has_issues = true;
        println!("Wallet files not in registry:");
        for address in &files_only {
            println!(
                "  - {} (wallet_{}.json exists but not registered)",
                address.yellow(),
                address
            );
        }
        println!();
    }

    // Report successful matches
    if !matching.is_empty() {
        println!("Properly registered wallets:");
        for address in &matching {
            println!("  - {}", address.green());
        }
        println!();
    }

    // Summary
    if has_issues {
        println!("{}", "Integrity issues found!".red().bold());
        println!("Consider:");
        if !registry_only.is_empty() {
            println!("- Remove orphaned registry entries or restore missing wallet files");
        }
        if !files_only.is_empty() {
            println!("- Register untracked wallet files or remove them if not needed");
        }
    } else if registry_wallets.is_empty() && file_wallets.is_empty() {
        println!(
            "{}",
            "No wallets found (this is normal for new installations)".blue()
        );
    } else {
        println!(
            "{}",
            "All wallets are properly registered and files exist!"
                .green()
                .bold()
        );
    }

    Ok(())
}

/// Import a wallet from a file path
pub fn import_wallet_from_file(
    file_path: &str,
    wallet_name: &str,
) -> Result<String, BridgeCliError> {
    let source_path = std::path::Path::new(file_path);

    if !source_path.exists() {
        return Err(BridgeCliError::WalletFileNotFound(file_path.to_string()));
    }

    if !source_path.is_file() {
        return Err(BridgeCliError::PathNotAFile(file_path.to_string()));
    }

    // Read and validate the wallet file
    let wallet_content = fs::read_to_string(source_path)?;
    let wallet_data: serde_json::Value = serde_json::from_str(&wallet_content)?;

    // Extract wallet address from the file content
    let wallet_address = wallet_data["address"]
        .as_str()
        .ok_or_else(|| BridgeCliError::MissingWalletAddress)?;

    let network = parse_network(
        wallet_data["network"]
            .as_str()
            .ok_or_else(|| BridgeCliError::MissingNetworkField)?,
    )?;

    // Validate that both wallet name and address don't already exist
    validate_wallet_availability(wallet_name, wallet_address, network)?;

    // Check if encrypted data exists (new format with separate encrypted fields)
    if !wallet_data["encrypted_mnemonic"].is_object() {
        return Err(BridgeCliError::MissingEncryptedMnemonicField);
    }

    // Check if destination wallet already exists BEFORE prompting for passphrase
    let storage_dir = get_storage_dir()?;
    let dest_wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_address));

    if dest_wallet_file.exists() {
        return Err(BridgeCliError::WalletAlreadyExists(
            wallet_address.to_string(),
        ));
    }

    println!("{}", "Passphrase Verification Required".yellow().bold());
    println!("To import this wallet, you must provide the correct passphrase to verify access.");

    // Prompt for passphrase to verify the user can decrypt the wallet
    let passphrase = prompt_unlock_passphrase()?;

    // Verify passphrase by attempting to decrypt the wallet data
    println!("Verifying passphrase...");

    // Parse encrypted mnemonic as EncryptedDataHex
    let encrypted_mnemonic_hex: encryption::EncryptedDataHex =
        serde_json::from_value(wallet_data["encrypted_mnemonic"].clone()).map_err(|e| {
            BridgeCliError::Eyre(eyre!("Failed to parse encrypted mnemonic structure: {}", e))
        })?;

    let encrypted_data = encryption::encrypted_data_from_hex(&encrypted_mnemonic_hex)
        .map_err(|e| BridgeCliError::Eyre(eyre!("Failed to parse encrypted mnemonic: {}", e)))?;

    // Try to decrypt mnemonic to verify passphrase
    match aes_decrypt_secure(&encrypted_data, &passphrase) {
        Ok(decrypted_mnemonic) => {
            // Additional validation: check if decrypted content looks like a valid mnemonic
            let mnemonic_str = decrypted_mnemonic.expose_secret();

            // Basic validation: should have words separated by spaces
            let words: Vec<&str> = mnemonic_str.split_whitespace().collect();
            if words.len() != MNEMONIC_WORD_COUNT {
                return Err(BridgeCliError::MnemonicParseError(format!(
                    "Invalid mnemonic length: expected {} words, got {}",
                    MNEMONIC_WORD_COUNT,
                    words.len()
                )));
            }

            // Validate wallet data based on import type
            if mnemonic_str == "IMPORTED_FROM_PRIVATE_KEY" {
                validate_private_key_import(&wallet_data, &passphrase, wallet_address)?;
            } else {
                validate_mnemonic_import(&decrypted_mnemonic, &wallet_data, wallet_address)?;
            }
        }
        Err(_) => {
            return Err(BridgeCliError::IncorrectPassphrase);
        }
    }

    // Storage directory was already created during the file existence check
    fs::create_dir_all(&storage_dir)?;

    // Create new wallet content instead of copying
    let network = wallet_data["network"]
        .as_str()
        .expect("network field checked above");
    let data_format = wallet_data["data_format"]
        .as_str()
        .unwrap_or("separate_encrypted_fields");

    // Extract and convert encrypted data from the original wallet
    let encrypted_mnemonic_hex: encryption::EncryptedDataHex =
        serde_json::from_value(wallet_data["encrypted_mnemonic"].clone()).map_err(|e| {
            BridgeCliError::Eyre(eyre!("Failed to parse encrypted mnemonic: {}", e))
        })?;

    let encrypted_private_key_hex: encryption::EncryptedDataHex =
        serde_json::from_value(wallet_data["encrypted_private_key"].clone()).map_err(|e| {
            BridgeCliError::Eyre(eyre!("Failed to parse encrypted private key: {}", e))
        })?;

    // Convert hex structures to EncryptedData
    let encrypted_mnemonic_data = encryption::encrypted_data_from_hex(&encrypted_mnemonic_hex)
        .map_err(|e| BridgeCliError::Eyre(eyre!("Failed to convert encrypted mnemonic: {}", e)))?;

    let encrypted_private_key_data =
        encryption::encrypted_data_from_hex(&encrypted_private_key_hex).map_err(|e| {
            BridgeCliError::Eyre(eyre!("Failed to convert encrypted private key: {}", e))
        })?;

    // Use store_wallet_data function for consistent storage
    let network_enum = parse_network(network)?;
    wallet_storage::store_wallet_data(
        wallet_address,
        network_enum,
        &encrypted_mnemonic_data,
        &encrypted_private_key_data,
        data_format,
        true,
        Some("file_import"),
        wallet_name,
    )?;

    Ok(wallet_address.to_string())
}

/// Import a wallet from a private key
pub fn import_wallet_from_private_key(
    network: Network,
    wallet_name: &str,
) -> Result<String, BridgeCliError> {
    let private_key = rpassword::prompt_password("Enter your private key (hex format): ")
        .map_err(|e| BridgeCliError::Eyre(eyre!("Failed to read private key: {}", e)))?;

    let mut private_key_bytes = hex::decode(private_key)
        .map_err(|e| BridgeCliError::Eyre(eyre!("Invalid private key hex format: {}", e)))?;

    if private_key_bytes.len() != 32 {
        private_key_bytes.zeroize();
        return Err(BridgeCliError::InvalidPrivateKey(
            "Private key must be exactly 32 bytes (64 hex characters)".to_string(),
        ));
    }

    let mut master_private_key = SecretKey::from_slice(&private_key_bytes).map_err(|e| {
        private_key_bytes.zeroize();
        BridgeCliError::InvalidPrivateKey(e.to_string())
    })?;
    private_key_bytes.zeroize();

    let keypair = Keypair::from_secret_key(&SECP, &master_private_key);
    let address = calculate_taproot_address(&keypair, network);

    // Check if address already exists
    validate_wallet_availability(wallet_name, &address.to_string(), network).inspect_err(|_| {
        master_private_key.non_secure_erase();
    })?;

    let passphrase = prompt_passphrase(false)?;

    let placeholder_mnemonic = SecureString::init_with(|| "IMPORTED_FROM_PRIVATE_KEY".to_string());

    let mut master_private_key_str = master_private_key.display_secret().to_string();
    let master_private_key_secure = SecureString::init_with(|| master_private_key_str.clone());

    master_private_key_str.zeroize();
    master_private_key.non_secure_erase();

    let encrypted_mnemonic = aes_encrypt_secure(&placeholder_mnemonic, &passphrase)
        .map_err(|e| BridgeCliError::PlaceholderMnemonicEncryptionFailed(e.to_string()))?;

    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .map_err(|e| BridgeCliError::PrivateKeyEncryptionFailed(e.to_string()))?;

    wallet_storage::store_wallet_data(
        &address.to_string(),
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
        "separate_encrypted_fields",
        true,
        Some("private_key_import"),
        wallet_name,
    )
    .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Failed to store wallet: {}", e)))?;

    println!(
        "{} Note: This wallet was imported from a private key, so no mnemonic phrase is available.",
        "INFO".yellow()
    );

    // Note: private_key_hex (SecureString), placeholder_mnemonic (SecureString),
    // master_private_key_secure (SecureString), and passphrase (SecretString)
    // will all be automatically zeroized when they go out of scope
    Ok(address.to_string())
}

pub fn show_private_key(wallet_name: &str, network: Network) -> Result<(), BridgeCliError> {
    // Check if wallet file exists before prompting for passphrase
    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_name));

    if !wallet_file.exists() {
        return Err(BridgeCliError::WalletNotFound(wallet_name.to_string()));
    }

    let passphrase = prompt_unlock_passphrase()?;

    let (mut keypair, _) = load_key_and_address(wallet_name, network, &passphrase)?;

    display_private_key_securely(&keypair.secret_key())?;

    keypair.non_secure_erase();

    Ok(())
}

#[cfg(test)]
pub mod tests {
    use passphrase::derive_key_from_passphrase;

    use super::*;

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
    fn test_key_derivation_parameters() {
        // This test is maintained for key derivation functionality
        let passphrase = SecureString::init_with(|| "test_passphrase".to_string());
        let salt = [1u8; 32];

        // Test with minimum secure parameters
        let key_min = derive_key_from_passphrase(&passphrase, &salt, 3, 1024, 1).unwrap();

        // Test with production parameters
        let key_prod = derive_key_from_passphrase(&passphrase, &salt, 3, 65_536, 4).unwrap();

        // Both should succeed but produce different keys due to different parameters
        assert_ne!(key_min.expose_secret(), key_prod.expose_secret());
    }
}
