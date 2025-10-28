//! Wallet management and Bitcoin operations for Clementine CLI.
//!
//! This module provides comprehensive wallet functionality:
//! - Creating new encrypted wallets with generated mnemonics
//! - Importing wallets from mnemonics, private keys, or files
//! - Secure storage and retrieval of wallet data
//! - Backup and export operations
//! - Wallet integrity verification and scanning
//!
//! ## Core Operations
//!
//! - **Wallet creation**: Generate new wallets with BIP-39 mnemonics
//! - **Import/export methods**: Support mnemonic, private key, and file imports, and wallet backup
//! - **Secure access**: All operations require passphrase authentication
//! - **Data integrity**: Registry and file consistency verification
//!

pub(crate) mod address;
mod encryption;
pub(crate) mod mnemonic;
pub(crate) mod passphrase;
pub(crate) mod wallet_storage;
pub(crate) mod wallet_utils;

pub use address::Purpose;
pub use address::parse_address;
pub use address::parse_taproot_address;
pub use address::print_all_wallets_with_addresses;
pub use address::should_not_have_purpose;

use bip39::Mnemonic;
use bitcoin::Network;
use bitcoin::address::NetworkChecked;
use bitcoin::address::NetworkUnchecked;
use eyre::eyre;
use secrecy::ExposeSecret;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::bitcoin_utils::SECP;
use crate::secure_types::SecureByteVec;
use crate::secure_types::SecureKeypair;
use crate::secure_types::SecureSecretKey;
use crate::secure_types::SecureString;
use crate::structs::TaprootAddressWithPrefix;
use crate::wallet::address::calculate_taproot_address;
use crate::wallet::address::generate_address_from_mnemonic;
use crate::wallet::encryption::{aes_decrypt_secure, aes_encrypt_secure};
use crate::wallet::mnemonic::derive_private_key_from_mnemonic;
use crate::wallet::mnemonic::generate_mnemonic;
use crate::wallet::mnemonic::load_mnemonic;
use crate::wallet::passphrase::prompt_passphrase;
use crate::wallet::wallet_storage::GenericWalletData;
use crate::wallet::wallet_storage::copy_wallet_file_to_destination;
use crate::wallet::wallet_storage::get_storage_dir_with_existence_check;
use crate::wallet::wallet_storage::get_wallets_from_registry;
use crate::wallet::wallet_utils::ensure_wallet_exists;
use crate::wallet::wallet_utils::load_key;
use crate::wallet::wallet_utils::{
    WalletValidationMode, parse_network, validate_wallet_availability,
};
use bitcoin::secp256k1::{Keypair, SecretKey};

use crate::errors::BridgeCliError;
use crate::wallet::mnemonic::MNEMONIC_WORD_COUNT;
use crate::wallet::wallet_utils::{
    parse_and_validate_imported_wallet, validate_mnemonic_import, validate_private_key_import,
};

pub fn create_encrypted_wallet(
    network: Network,
    label: String,
    purpose: Purpose,
    passphrase: SecureString,
) -> Result<(TaprootAddressWithPrefix<NetworkChecked>, Mnemonic), BridgeCliError> {
    // Generate mnemonic
    let mnemonic = generate_mnemonic()?;

    // Generate address from mnemonic using helper function
    let address =
        address::generate_address_from_mnemonic(&mnemonic, network, purpose).map_err(|e| {
            tracing::error!("Error generating address from mnemonic: {}", e);
            BridgeCliError::AddressGenerationFromMnemonicFailed
        })?;

    // Validate that both wallet name and address don't already exist
    validate_wallet_availability(Some(&label), Some(&address), WalletValidationMode::Both)?;

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

    Ok((address, mnemonic))
}

/// Backup a wallet file to a specified destination
pub fn backup_wallet(
    address: &TaprootAddressWithPrefix<NetworkUnchecked>,
    destination_path: &Path,
) -> Result<PathBuf, BridgeCliError> {
    ensure_wallet_exists(address)?;

    let final_dest = copy_wallet_file_to_destination(address, destination_path)?;

    // Set secure file permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&final_dest)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&final_dest, perms)?;
    }

    Ok(final_dest)
}

/// Import a wallet using secure mnemonic input (step-by-step) and password creation
pub fn import_wallet_from_mnemonic(
    network: Network,
    label: &str,
    purpose: Purpose,
    mnemonic: Mnemonic,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    validate_wallet_availability(Some(label), None, WalletValidationMode::Label)?;

    // Generate address from mnemonic using helper function
    let address = generate_address_from_mnemonic(&mnemonic, network, purpose).map_err(|e| {
        tracing::error!("Error generating address from mnemonic: {}", e);
        BridgeCliError::AddressGenerationFromMnemonicFailed
    })?;

    validate_wallet_availability(None, Some(&address), WalletValidationMode::Address)?;
    let _address_str = address.address_with_prefix();

    let passphrase = prompt_passphrase(true)?;

    let master_private_key_secure = derive_private_key_from_mnemonic(&mnemonic).map_err(|e| {
        tracing::error!("Error deriving private key from mnemonic: {}", e);
        BridgeCliError::PrivateKeyDerivationFromMnemonicFailed
    })?;

    let mnemonic_secure: SecureString = SecureString::init_with(|| mnemonic.to_string());

    let encrypted_mnemonic = aes_encrypt_secure(&mnemonic_secure, &passphrase).map_err(|e| {
        tracing::error!("Error encrypting mnemonic: {}", e);
        BridgeCliError::MnemonicEncryptionFailed
    })?;

    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .map_err(|e| {
            tracing::error!("Error encrypting private key: {}", e);
            BridgeCliError::PrivateKeyEncryptionFailed
        })?;

    wallet_storage::store_wallet_data(
        &address,
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
        true,
        Some("mnemonic_import"),
        label,
    )
    .map_err(|e| {
        tracing::error!("Error storing wallet data: {}", e);
        BridgeCliError::WalletStorageFailed
    })?;

    Ok(address)
}

/// Import a wallet from a file path
pub fn import_wallet_from_file(
    file_path: &Path,
    label: Option<&str>,
    passphrase: SecureString,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Parse and validate the wallet file using helper function
    let wallet_data = parse_and_validate_imported_wallet(file_path, label)?;

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
                tracing::error!(
                    "Decrypted mnemonic has invalid word count: expected {}, got {}",
                    MNEMONIC_WORD_COUNT,
                    words.len()
                );
                return Err(BridgeCliError::MnemonicParseError);
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

    let wallet_address = TaprootAddressWithPrefix::from_string_with_prefix(
        &wallet_data.address_with_prefix,
        network,
    )?;

    let label = if let Some(lbl) = label {
        lbl
    } else {
        &wallet_data.label
    };

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
    private_key: SecureString,
    passphrase: SecureString,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    let private_key_bytes = SecureByteVec::new(Box::new(
        hex::decode(private_key.expose_secret())
            .map_err(|e| BridgeCliError::Eyre(eyre!("Invalid private key hex format: {}", e)))?,
    ));

    if private_key_bytes.expose_secret().len() != 32 {
        return Err(BridgeCliError::InvalidPrivateKey(
            "Private key must be exactly 32 bytes (64 hex characters)".to_string(),
        ));
    }

    let master_private_key = SecureSecretKey::new(
        SecretKey::from_slice(private_key_bytes.expose_secret()).map_err(|e| {
            tracing::error!("Error parsing private key: {}", e);
            BridgeCliError::Eyre(eyre!("Failed to parse private key"))
        })?,
    );

    let keypair = SecureKeypair::new(Keypair::from_secret_key(
        &SECP,
        master_private_key.as_ref_inner(),
    ));
    let address = calculate_taproot_address(&keypair, network);

    let address = TaprootAddressWithPrefix::new(address, purpose)?;

    let placeholder_mnemonic = SecureString::init_with(|| "IMPORTED_FROM_PRIVATE_KEY".to_string());

    let master_private_key_secure = SecureString::init_with(|| {
        master_private_key
            .as_ref_inner()
            .display_secret()
            .to_string()
    });

    let encrypted_mnemonic =
        aes_encrypt_secure(&placeholder_mnemonic, &passphrase).map_err(|e| {
            tracing::error!("Error encrypting placeholder mnemonic: {}", e);
            BridgeCliError::PlaceholderMnemonicEncryptionFailed
        })?;

    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .map_err(|e| {
            tracing::error!("Error encrypting private key: {}", e);
            BridgeCliError::PrivateKeyEncryptionFailed
        })?;

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

pub fn get_mnemonic_from_wallet<T>(
    address: &TaprootAddressWithPrefix<T>,
    passphrase: &SecureString,
) -> Result<Mnemonic, BridgeCliError>
where
    T: bitcoin::address::NetworkValidation,
    bitcoin::Address<T>: crate::structs::AddrDisplay,
{
    ensure_wallet_exists(address)?;

    let mnemonic = load_mnemonic(address, passphrase)?;

    Ok(mnemonic)
}

pub fn get_private_key_from_wallet<T>(
    address: &TaprootAddressWithPrefix<T>,
    passphrase: &SecureString,
) -> Result<SecureSecretKey, BridgeCliError>
where
    T: bitcoin::address::NetworkValidation,
    bitcoin::Address<T>: crate::structs::AddrDisplay,
{
    ensure_wallet_exists(address)?;

    let keypair = load_key(address, passphrase)?;

    Ok(keypair.secret_key())
}

pub fn scan_wallet_files()
-> Result<HashSet<TaprootAddressWithPrefix<NetworkUnchecked>>, BridgeCliError> {
    let storage_dir = get_storage_dir_with_existence_check()?;
    let mut file_wallets: HashSet<TaprootAddressWithPrefix<NetworkUnchecked>> = HashSet::new();

    if !storage_dir.exists() {
        return Ok(file_wallets);
    }

    for entry in fs::read_dir(storage_dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        // Skip non-wallet files
        if !file_name_str.starts_with("wallet_")
            || !file_name_str.ends_with(".json")
            || file_name_str == "wallets.json"
        {
            continue;
        }

        let wallet_file_path = entry.path();
        let wallet_content = match fs::read_to_string(&wallet_file_path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to read wallet file {}: {}",
                    file_name_str, e
                );
                continue;
            }
        };

        match serde_json::from_str::<GenericWalletData>(&wallet_content) {
            Ok(wallet_data) => {
                match TaprootAddressWithPrefix::from_string_with_prefix_unchecked(
                    &wallet_data.address_with_prefix,
                ) {
                    Ok(addr) => {
                        file_wallets.insert(addr);
                    }
                    Err(e) => eprintln!(
                        "Warning: Failed to parse address in wallet file {}: {}",
                        file_name_str, e
                    ),
                }
            }
            Err(e) => eprintln!(
                "Warning: Failed to parse wallet file {}: {}",
                file_name_str, e
            ),
        }
    }

    Ok(file_wallets)
}

pub fn get_registry_wallet_set()
-> Result<HashSet<TaprootAddressWithPrefix<NetworkUnchecked>>, BridgeCliError> {
    let registry_wallet_data = get_wallets_from_registry()?;

    let mut wallet_set: HashSet<TaprootAddressWithPrefix<NetworkUnchecked>> = HashSet::new();

    for (_address, wallet_value) in registry_wallet_data {
        wallet_set.insert(TaprootAddressWithPrefix::from_string_with_prefix_unchecked(
            &wallet_value.addres_with_prefix,
        )?);
    }

    Ok(wallet_set)
}
