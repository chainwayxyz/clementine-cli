pub(crate) mod address;
mod encryption;
mod mnemonic;
pub(crate) mod passphrase;
mod wallet_storage;
pub(crate) mod wallet_utils;

pub use address::Purpose;
pub use address::parse_address;
pub use address::print_all_wallets_with_addresses;

use bitcoin::Network;
use bitcoin::address::NetworkChecked;
use bitcoin::address::NetworkUnchecked;
use colored::Colorize;
use eyre::eyre;
use secrecy::ExposeSecret;
use std::fs;
use std::path::PathBuf;

use crate::bitcoin_utils::{SECP, calculate_taproot_address};
use crate::structs::TaprootAddressWithPrefix;
use crate::wallet::address::generate_address_from_mnemonic;
use crate::wallet::encryption::{aes_decrypt_secure, aes_encrypt_secure};
use crate::wallet::mnemonic::generate_mnemonic;
use crate::wallet::mnemonic::{derive_private_key_from_mnemonic, prompt_mnemonic};
use crate::wallet::passphrase::prompt_passphrase;
use crate::wallet::wallet_storage::{get_registry_wallet_set, scan_wallet_files};
use crate::wallet::wallet_utils::ensure_wallet_exists;
use crate::wallet::wallet_utils::get_mnemonic_from_wallet;
use crate::wallet::wallet_utils::get_private_key_from_wallet;
use crate::wallet::wallet_utils::{
    WalletValidationMode, parse_network, validate_wallet_availability,
};
use bitcoin::secp256k1::{Keypair, SecretKey};

use crate::errors::BridgeCliError;
use crate::structs::{SecureByteVec, SecureKeypair, SecureSecretKey, SecureString};
use crate::wallet::mnemonic::MNEMONIC_WORD_COUNT;
use crate::wallet::passphrase::prompt_unlock_passphrase;
use crate::wallet::wallet_storage::get_storage_dir;
use crate::wallet::wallet_utils::{
    parse_and_validate_imported_wallet, report_integrity_results, validate_mnemonic_import,
    validate_private_key_import,
};

pub fn create_encrypted_wallet_with_address(
    network: Network,
    label: String,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Generate mnemonic
    let mnemonic = generate_mnemonic()?;

    // Generate address from mnemonic using helper function
    let address = address::generate_address_from_mnemonic(&mnemonic, network, purpose)
        .map_err(|e| BridgeCliError::AddressGenerationFromMnemonicFailed(e.to_string()))?;

    // Validate that both wallet name and address don't already exist
    validate_wallet_availability(Some(&label), Some(&address), WalletValidationMode::Both)?;

    // Prompt for passphrase
    let passphrase = prompt_passphrase(true)?;

    // Encrypt mnemonic and private key separately with different nonces
    let master_private_key_secure = derive_private_key_from_mnemonic(&mnemonic)?;

    let mnemonic_secure: SecureString = SecureString::init_with(|| mnemonic.to_string());

    let encrypted_mnemonic = aes_encrypt_secure(&mnemonic_secure, &passphrase)?;
    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)?;

    // Store encrypted wallet with separate encrypted fields
    wallet_storage::store_wallet_data(
        &address,
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
        false,
        None,
        &label,
    )?;

    // Display the mnemonic securely to the user
    crate::secure_display::display_mnemonic_securely(&mnemonic)?;

    Ok(address)
}

/// Backup a wallet file to a specified destination
pub fn backup_wallet(

    address_with_prefix: &str,
    destination_path: &str,
) -> Result<(TaprootAddressWithPrefix<NetworkUnchecked>, PathBuf), BridgeCliError> {
    let address = TaprootAddressWithPrefix::from_string_with_prefix_unchecked(address_with_prefix)?;

    ensure_wallet_exists(&address)?;

    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address.address_without_prefix()));

    // Parse the destination path
    let dest_path = std::path::Path::new(destination_path);

    // If destination is a directory, create the filename
    let final_dest = if dest_path.is_dir() {
        dest_path.join(format!("wallet_{}.json", address.address_without_prefix()))
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

    Ok((address, final_dest))
}

/// Import a wallet using secure mnemonic input (step-by-step) and password creation
pub fn import_wallet_from_mnemonic(
    network: Network,
    label: &str,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    validate_wallet_availability(Some(label), None, WalletValidationMode::Label)?;

    println!("{}", "Import Wallet with Mnemonic".blue().bold());

    // Prompt for mnemonic securely (word by word)
    let mnemonic = prompt_mnemonic()?;

    // Generate address from mnemonic using helper function
    let address = generate_address_from_mnemonic(&mnemonic, network, purpose)
        .map_err(|e| BridgeCliError::AddressGenerationFromMnemonicFailed(e.to_string()))?;

    validate_wallet_availability(None, Some(&address), WalletValidationMode::Address)?;
    let _address_str = address.address_with_prefix();

    let passphrase = prompt_passphrase(true)?;

    let master_private_key_secure = derive_private_key_from_mnemonic(&mnemonic)
        .map_err(|e| BridgeCliError::PrivateKeyDerivationFromMnemonicFailed(e.to_string()))?;

    let mnemonic_secure: SecureString = SecureString::init_with(|| mnemonic.to_string());

    let encrypted_mnemonic = aes_encrypt_secure(&mnemonic_secure, &passphrase)
        .map_err(|e| BridgeCliError::MnemonicEncryptionFailed(e.to_string()))?;

    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .map_err(|e| BridgeCliError::PrivateKeyEncryptionFailed(e.to_string()))?;

    wallet_storage::store_wallet_data(
        &address,
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
        true,
        Some("mnemonic_import"),
        label,
    )
    .map_err(|e| BridgeCliError::WalletStorageFailed(e.to_string()))?;

    Ok(address)
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
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Parse and validate the wallet file using helper function
    let wallet_data = parse_and_validate_imported_wallet(file_path, label)?;

    // Prompt for passphrase to verify the user can decrypt the wallet
    let passphrase = prompt_unlock_passphrase()?;

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
                validate_private_key_import(
                    &wallet_data,
                    &passphrase,
                    &wallet_data.address_with_prefix,
                )?;
            } else {
                validate_mnemonic_import(&decrypted_mnemonic, &wallet_data)?;
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

    let wallet_address = TaprootAddressWithPrefix::from_string_with_prefix(
        &wallet_data.address_with_prefix,
        network,
    )?;

    // Use store_wallet_data function for consistent storage
    wallet_storage::store_wallet_data(
        &wallet_address,
        network,
        &encrypted_mnemonic_data,
        &encrypted_private_key_data,
        true,
        Some("file_import"),
        label,
    )?;

    Ok(wallet_address)
}

/// Import a wallet from a private key
pub fn import_wallet_from_private_key(
    network: Network,
    label: &str,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
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

    let address = TaprootAddressWithPrefix::new(address, purpose)?;

    let _address_str = address.address_with_prefix();

    // Check if address already exists
    validate_wallet_availability(None, Some(&address), WalletValidationMode::Address)?;

    let passphrase = prompt_passphrase(true)?;

    let placeholder_mnemonic = SecureString::init_with(|| "IMPORTED_FROM_PRIVATE_KEY".to_string());

    let master_private_key_secure =
        SecureString::init_with(|| master_private_key.as_ref().display_secret().to_string());

    let encrypted_mnemonic = aes_encrypt_secure(&placeholder_mnemonic, &passphrase)
        .map_err(|e| BridgeCliError::PlaceholderMnemonicEncryptionFailed(e.to_string()))?;

    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .map_err(|e| BridgeCliError::PrivateKeyEncryptionFailed(e.to_string()))?;

    wallet_storage::store_wallet_data(
        &address,
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
        true,
        Some("private_key_import"),
        label,
    )
    .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Failed to store wallet: {}", e)))?;

    Ok(address)
}

/// Show mnemonic securely for a wallet
pub fn show_mnemonic(
    address: &TaprootAddressWithPrefix<bitcoin::address::NetworkUnchecked>,
) -> Result<(), BridgeCliError> {
    let mnemonic = get_mnemonic_from_wallet(address)?;
    crate::secure_display::display_mnemonic_securely(&mnemonic)?;
    Ok(())
}

/// Show private key securely for a wallet
pub fn show_private_key(
    address: &TaprootAddressWithPrefix<bitcoin::address::NetworkUnchecked>,
) -> Result<(), BridgeCliError> {
    let private_key = get_private_key_from_wallet(address)?;
    crate::secure_display::display_private_key_securely(&private_key)?;
    Ok(())
}
