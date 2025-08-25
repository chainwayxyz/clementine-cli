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

use crate::bitcoin_utils::{SECP, calculate_taproot_address};
use crate::wallet::address::generate_address_from_mnemonic_secure;
use crate::wallet::wallet_storage::{load_wallet_data, scan_wallet_files};
use crate::wallet::wallet_utils::{address_exists, load_key, parse_network};
use bitcoin::secp256k1::{Keypair, SecretKey};

use crate::errors::BridgeCliError;
use crate::secure_display::{display_mnemonic_securely, display_private_key_securely};
use crate::structs::{SecureByteVec, SecureKeypair, SecureSecretKey, SecureString};
use encryption::{aes_decrypt_secure, aes_encrypt_secure};
use mnemonic::{
    MNEMONIC_WORD_COUNT, derive_private_key_from_mnemonic_secure, generate_mnemonic_secure,
    prompt_mnemonic_secure,
};
use passphrase::{prompt_passphrase, prompt_unlock_passphrase};
use wallet_storage::get_storage_dir;
use wallet_storage::remove_wallet_from_registry;
use wallet_utils::{
    WalletValidationMode, parse_and_validate_imported_wallet, report_integrity_results,
    validate_mnemonic_import, validate_private_key_import, validate_wallet_availability,
};

pub fn create_encrypted_wallet_with_address(
    network: Network,
    label: String,
) -> Result<(), BridgeCliError> {
    // Generate mnemonic
    let secure_mnemonic = generate_mnemonic_secure()?;

    // Generate address from mnemonic using helper function
    let address = address::generate_address_from_mnemonic_secure(&secure_mnemonic, network)
        .map_err(|e| BridgeCliError::AddressGenerationFromMnemonicFailed(e.to_string()))?;

    let address_str = address.to_string();

    // Validate that both wallet name and address don't already exist
    validate_wallet_availability(
        Some(&label),
        Some(&address.to_string()),
        WalletValidationMode::Both,
    )?;

    // Prompt for passphrase
    let passphrase = prompt_passphrase(true)?;

    // Encrypt mnemonic and private key separately with different nonces
    let master_private_key_secure = derive_private_key_from_mnemonic_secure(&secure_mnemonic)?;

    let encrypted_mnemonic = aes_encrypt_secure(&secure_mnemonic, &passphrase)?;
    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)?;

    // Store encrypted wallet with separate encrypted fields
    wallet_storage::store_wallet_data(
        &address_str,
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
        false,
        None,
        &label,
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

    Ok(())
}

pub fn delete_wallet(address: &str) -> Result<(), BridgeCliError> {
    // Check if wallet exists
    if !address_exists(address)? {
        return Err(BridgeCliError::WalletNotFound(address.to_string()));
    }

    let wallet_data = load_wallet_data(address)?;
    let address = wallet_data.address.as_str();

    println!("{}", "Wallet Deletion".red().bold());
    println!(
        "You are about to delete the wallet with address: {}",
        address.yellow()
    );
    println!("{}", "This action cannot be undone!".red().bold());
    println!("Make sure you have backed up your wallet before proceeding.");
    println!();

    print!("Type 'DELETE' to confirm deletion: ");
    io::stdout().flush()?;
    let mut confirmation = String::new();
    io::stdin().read_line(&mut confirmation)?;

    if confirmation.trim() != "DELETE" {
        println!("Deletion cancelled.");
        return Ok(());
    }

    println!("Deleting wallet...");

    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    fs::remove_file(&wallet_file)?;
    println!("Wallet file deleted: {}", wallet_file.display());

    // Remove from wallets.json registry
    if remove_wallet_from_registry(address)? {
        println!("Wallet removed from registry");
    } else {
        println!("{}", "Wallet was not found in registry".yellow());
    }

    println!("{}", "Wallet deleted successfully!".green().bold());

    Ok(())
}

/// Backup a wallet file to a specified destination
pub fn backup_wallet(address: &str, destination_path: &str) -> Result<(), BridgeCliError> {
    if !address_exists(address)? {
        return Err(BridgeCliError::WalletNotFound(address.to_string()));
    }

    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    // Parse the destination path
    let dest_path = std::path::Path::new(destination_path);

    // If destination is a directory, create the filename
    let final_dest = if dest_path.is_dir() {
        dest_path.join(format!("wallet_{}.json", address))
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
        perms.set_mode(0o600);
        fs::set_permissions(&final_dest, perms)?;
    }

    println!(
        "{} Wallet '{}' backed up successfully to: {}",
        "SUCCESS".green(),
        address.cyan(),
        final_dest.display().to_string().yellow()
    );

    Ok(())
}

/// Import a wallet using secure mnemonic input (step-by-step) and password creation
pub fn import_wallet_from_mnemonic(
    network: Network,
    label: &str,
) -> Result<String, BridgeCliError> {
    validate_wallet_availability(Some(label), None, WalletValidationMode::Label)?;

    println!("{}", "Import Wallet with Mnemonic".blue().bold());

    // Prompt for mnemonic securely (word by word)
    println!("{}", "Step 1: Enter Mnemonic Phrase".yellow().bold());
    let secure_mnemonic = prompt_mnemonic_secure()?;

    // Generate address from mnemonic using helper function
    let address = generate_address_from_mnemonic_secure(&secure_mnemonic, network)
        .map_err(|e| BridgeCliError::AddressGenerationFromMnemonicFailed(e.to_string()))?;

    validate_wallet_availability(
        None,
        Some(&address.to_string()),
        WalletValidationMode::Address,
    )?;

    println!();
    println!("Mnemonic processed successfully!");
    println!("Derived address: {}", address.to_string().green());
    println!();

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
        true,
        Some("mnemonic_import"),
        label,
    )
    .map_err(|e| BridgeCliError::WalletStorageFailed(e.to_string()))?;

    Ok(address.to_string())
}

pub fn get_registry_wallet_set() -> Result<HashSet<String>, BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    let wallets_file = storage_dir.join("wallets.json");

    let registry_wallet_data = if wallets_file.exists() {
        let wallets_content = fs::read_to_string(&wallets_file)?;
        let wallets: HashMap<String, serde_json::Value> = serde_json::from_str(&wallets_content)
            .map_err(|e| BridgeCliError::WalletsJsonParseFailed(e.to_string()))?;
        wallets
    } else {
        println!("wallets.json not found - no registered wallets");
        HashMap::new()
    };

    let mut wallet_map: HashSet<String> = HashSet::new();

    for (_name, wallet_value) in registry_wallet_data {
        match serde_json::from_value::<crate::wallet::wallet_storage::GenericWalletData>(
            wallet_value,
        ) {
            Ok(wallet_struct) => {
                wallet_map.insert(wallet_struct.address);
            }
            Err(e) => {
                eprintln!("Warning: Failed to parse wallet registry entry: {}", e);
            }
        }
    }

    Ok(wallet_map)
}

pub fn verify_wallet_integrity() -> Result<(), BridgeCliError> {
    let storage_dir = get_storage_dir()?;

    println!("{}", "Verifying Wallet Integrity".blue().bold());
    println!("Storage directory: {}", storage_dir.display());
    println!();

    // Load registered wallets from wallets.json
    let registry_wallets = get_registry_wallet_set()?;

    // Scan for actual wallet files in storage directory
    let file_wallets = scan_wallet_files()?;

    // Report integrity results
    report_integrity_results(&registry_wallets, &file_wallets);

    Ok(())
}

/// Import a wallet from a file path
pub fn import_wallet_from_file(
    file_path: &str,
    label: Option<&str>,
) -> Result<String, BridgeCliError> {
    // Parse and validate the wallet file using helper function
    let wallet_data = parse_and_validate_imported_wallet(file_path, label)?;

    println!("{}", "Passphrase Verification Required".yellow().bold());
    println!("To import this wallet, you must provide the correct passphrase to verify access.");

    // Prompt for passphrase to verify the user can decrypt the wallet
    let passphrase = prompt_unlock_passphrase()?;

    // Verify passphrase by attempting to decrypt the wallet data
    println!("Verifying passphrase...");

    let encrypted_mnemonic_hex = wallet_data.encrypted_mnemonic.as_ref().unwrap();
    let encrypted_data = encryption::encrypted_data_from_hex(encrypted_mnemonic_hex)
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
                validate_private_key_import(&wallet_data, &passphrase, &wallet_data.address)?;
            } else {
                validate_mnemonic_import(&decrypted_mnemonic, &wallet_data, &wallet_data.address)?;
            }
        }
        Err(_) => {
            return Err(BridgeCliError::IncorrectPassphrase);
        }
    }

    // Convert encrypted data from the original wallet
    let encrypted_mnemonic_data =
        encryption::encrypted_data_from_hex(wallet_data.encrypted_mnemonic.as_ref().unwrap())
            .map_err(|e| {
                BridgeCliError::Eyre(eyre!("Failed to convert encrypted mnemonic: {}", e))
            })?;

    let encrypted_private_key_data =
        encryption::encrypted_data_from_hex(wallet_data.encrypted_private_key.as_ref().unwrap())
            .map_err(|e| {
                BridgeCliError::Eyre(eyre!("Failed to convert encrypted private key: {}", e))
            })?;

    let network = parse_network(&wallet_data.network)?;

    let label = if let Some(lbl) = label {
        lbl
    } else {
        &wallet_data.label
    };

    // Use store_wallet_data function for consistent storage
    wallet_storage::store_wallet_data(
        &wallet_data.address,
        network,
        &encrypted_mnemonic_data,
        &encrypted_private_key_data,
        true,
        Some("file_import"),
        label,
    )?;

    Ok(wallet_data.address)
}

/// Import a wallet from a private key
pub fn import_wallet_from_private_key(
    network: Network,
    label: &str,
) -> Result<String, BridgeCliError> {
    validate_wallet_availability(Some(label), None, WalletValidationMode::Label)?;

    let private_key_input = rpassword::prompt_password("Enter your private key (hex format): ")
        .map_err(|e| BridgeCliError::Eyre(eyre!("Failed to read private key: {}", e)))?;

    // Immediately secure the private key input
    let secure_private_key = SecureString::init_with(|| private_key_input);

    let private_key_bytes = SecureByteVec::new(Box::new(
        hex::decode(secure_private_key.expose_secret())
            .map_err(|e| BridgeCliError::Eyre(eyre!("Invalid private key hex format: {}", e)))?,
    ));

    if private_key_bytes.expose_secret().len() != 32 {
        return Err(BridgeCliError::InvalidPrivateKey(
            "Private key must be exactly 32 bytes (64 hex characters)".to_string(),
        ));
    }

    let master_private_key = SecureSecretKey::new(
        SecretKey::from_slice(private_key_bytes.expose_secret())
            .map_err(|e| BridgeCliError::InvalidPrivateKey(e.to_string()))?,
    );

    let keypair = SecureKeypair::new(Keypair::from_secret_key(&SECP, master_private_key.as_ref()));
    let address = calculate_taproot_address(&keypair, network);

    // Check if address already exists
    validate_wallet_availability(
        None,
        Some(&address.to_string()),
        WalletValidationMode::Address,
    )?;

    let passphrase = prompt_passphrase(false)?;

    let placeholder_mnemonic = SecureString::init_with(|| "IMPORTED_FROM_PRIVATE_KEY".to_string());

    let master_private_key_secure =
        SecureString::init_with(|| master_private_key.as_ref().display_secret().to_string());

    let encrypted_mnemonic = aes_encrypt_secure(&placeholder_mnemonic, &passphrase)
        .map_err(|e| BridgeCliError::PlaceholderMnemonicEncryptionFailed(e.to_string()))?;

    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .map_err(|e| BridgeCliError::PrivateKeyEncryptionFailed(e.to_string()))?;

    wallet_storage::store_wallet_data(
        &address.to_string(),
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
        true,
        Some("private_key_import"),
        label,
    )
    .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Failed to store wallet: {}", e)))?;

    println!(
        "{} Note: This wallet was imported from a private key, so no mnemonic phrase is available.",
        "INFO".yellow()
    );

    Ok(address.to_string())
}

pub fn show_private_key(address: &str) -> Result<(), BridgeCliError> {
    // Check if wallet file exists before prompting for passphrase
    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    if !wallet_file.exists() {
        return Err(BridgeCliError::WalletNotFound(address.to_string()));
    }

    let passphrase = prompt_unlock_passphrase()?;

    let keypair = load_key(address, &passphrase)?;

    display_private_key_securely(&keypair.secret_key())?;

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
