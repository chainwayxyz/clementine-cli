//! Wallet validation, key management, and utility functions for Clementine CLI.
//!
//! This module provides essential wallet operations and validation:
//! - Loading and validating encrypted keys from storage
//! - Network parsing and address validation
//! - Wallet existence and availability checking
//! - Import validation for mnemonics and private keys
//! - Wallet integrity reporting and verification
//!
//! ## Validation Features
//!
//! - **Wallet availability**: Checks for label and address conflicts
//! - **Import validation**: Verifies mnemonic and private key imports
//! - **Address derivation**: Confirms imported data matches addresses
//! - **Integrity checks**: Reports registry and file consistency
//!

use crate::errors::BridgeCliError;
use crate::secure_types::SecureKeypair;
use crate::secure_types::SecureSecretKey;
use crate::secure_types::SecureString;
use crate::sqlite_db::sqlite_client::SqliteDb;
use crate::sqlite_db::wallet_db::WalletExport;
use crate::sqlite_db::wallet_db::{WalletData, WalletTable};
use crate::structs::AddrDisplay;
use crate::structs::TaprootAddressWithPrefix;
use crate::wallet::Purpose;
use crate::wallet::address::calculate_taproot_address;
use crate::wallet::address::generate_address_from_mnemonic;
use crate::wallet::encryption::aes_decrypt_secure;
use crate::wallet::wallet_storage::load_wallet_data;
use bip39::Mnemonic;
use bitcoin::Network;
use bitcoin::address::NetworkChecked;
use bitcoin::address::NetworkValidation;
use bitcoin::key::Keypair;
use bitcoin::secp256k1::Secp256k1;
use bitcoin::secp256k1::SecretKey;
use eyre::Context;
use eyre::eyre;
use secrecy::ExposeSecret;
use std::path::Path;
use std::str::FromStr;

/// Securely load a key from wallet storage and check address validity - always requires a passphrase
pub(crate) async fn load_key<T>(
    address: &TaprootAddressWithPrefix<T>,
    passphrase: &SecureString,
) -> Result<SecureKeypair, BridgeCliError>
where
    T: NetworkValidation + Clone,
    bitcoin::Address<T>: AddrDisplay,
{
    ensure_wallet_exists(address)?;

    let wallet_data = load_wallet_data(address).await?.ok_or_else(|| {
        BridgeCliError::Eyre(eyre::eyre!(
            "Wallet data not found for address {}",
            address.address_with_prefix()
        ))
    })?;

    // Load the encrypted private key
    let encrypted_private_key = wallet_data
        .encrypted_private_key
        .ok_or_else(|| BridgeCliError::NoEncryptedPrivateKeyFound)?;

    let encrypted_data =
        crate::wallet::encryption::encrypted_data_from_hex(&encrypted_private_key)?;
    let decrypted_key = aes_decrypt_secure(&encrypted_data, passphrase)?;

    let secp = Secp256k1::new();
    let secret_key = SecureSecretKey::new(SecretKey::from_str(decrypted_key.expose_secret())?);

    let keypair = Keypair::from_secret_key(&secp, secret_key.as_ref_inner());
    let secure_keypair = SecureKeypair::new(keypair);

    Ok(secure_keypair)
}

/// Helper function to validate mnemonic imports during wallet import
pub(crate) fn validate_mnemonic_import(
    decrypted_mnemonic: &SecureString,
    wallet_data: &WalletData,
) -> Result<(), BridgeCliError> {
    let network = wallet_data.network;

    let mnemonic = Mnemonic::parse(decrypted_mnemonic.expose_secret()).map_err(|e| {
        tracing::error!("Error parsing mnemonic: {}", e);
        BridgeCliError::MnemonicValidationFailed
    })?;

    let wallet_address = wallet_data.address.clone();

    // Generate address from mnemonic to verify it matches
    match generate_address_from_mnemonic(&mnemonic, network, wallet_address.purpose) {
        Ok(derived_address) => {
            if derived_address.address != wallet_address.address {
                return Err(BridgeCliError::AddressMismatch);
            }
        }
        Err(e) => {
            tracing::error!("Error generating address from mnemonic: {}", e);
            return Err(BridgeCliError::AddressGenerationFromMnemonicFailed);
        }
    }

    Ok(())
}

/// Helper function to parse network string into Network enum
pub(crate) fn parse_network(network_str: &str) -> Result<Network, BridgeCliError> {
    match network_str {
        "testnet4" => Ok(Network::Testnet4),
        "regtest" => Ok(Network::Regtest),
        "signet" => Ok(Network::Signet),
        "bitcoin" => Ok(Network::Bitcoin),
        rest => Err(BridgeCliError::UnsupportedNetwork(
            Network::from_str(rest).wrap_err("Network is not a valid Bitcoin network name")?,
        )),
    }
}

/// Helper function to validate private key imports during wallet import
pub(crate) fn validate_private_key_import(
    wallet_data: &WalletData,
    passphrase: &SecureString,
    wallet_address: &str,
) -> Result<(), BridgeCliError> {
    if let Some(encrypted_private_key_hex) = &wallet_data.encrypted_private_key {
        let encrypted_private_data = crate::wallet::encryption::encrypted_data_from_hex(
            encrypted_private_key_hex,
        )
        .map_err(|e| BridgeCliError::Eyre(eyre!("Failed to parse encrypted private key: {}", e)))?;

        // Decrypt and validate the private key
        match aes_decrypt_secure(&encrypted_private_data, passphrase) {
            Ok(decrypted_private_key) => {
                let network = parse_network(&wallet_data.network.to_string())?;

                // Validate the private key format and derive address to verify
                match SecretKey::from_str(decrypted_private_key.expose_secret()) {
                    Ok(private_key) => {
                        let secure_secret_key = SecureSecretKey::new(private_key);
                        let keypair = SecureKeypair::new(Keypair::from_secret_key(
                            &crate::bitcoin_utils::SECP,
                            secure_secret_key.as_ref_inner(),
                        ));
                        let derived_address = calculate_taproot_address(&keypair, network);

                        if derived_address.to_string() != wallet_address {
                            return Err(BridgeCliError::AddressMismatch);
                        }
                    }
                    Err(_) => {
                        return Err(BridgeCliError::InvalidPrivateKey(
                            "Invalid private key format".to_string(),
                        ));
                    }
                }
            }
            Err(_) => {
                return Err(BridgeCliError::IncorrectPassphrase);
            }
        }
    } else {
        return Err(BridgeCliError::MissingEncryptedPrivateKeyField);
    }

    Ok(())
}

pub(crate) fn label_exists(label: &str) -> Result<bool, BridgeCliError> {
    tokio::runtime::Handle::current().block_on(async move {
        let db = SqliteDb::open_with_schema().await?;

        WalletTable::label_exists(db.pool(), label).await
    })
}

pub(crate) fn address_exists<T>(
    address: &TaprootAddressWithPrefix<T>,
) -> Result<bool, BridgeCliError>
where
    T: bitcoin::address::NetworkValidation,
    bitcoin::Address<T>: AddrDisplay,
{
    tokio::runtime::Handle::current().block_on(async move {
        let db = SqliteDb::open_with_schema().await?;

        WalletTable::address_exists(db.pool(), address).await
    })
}

/// Validation options for wallet creation and import operations
#[derive(Debug)]
pub enum WalletValidationMode {
    /// Check if wallet name already exists
    Label,
    /// Check if address already exists for the given network
    Address,
    /// Check both wallet name and address
    Both,
}

/// Combined validation function to check for conflicts during wallet operations
pub(crate) fn validate_wallet_availability(
    label: Option<&str>,
    address: Option<&TaprootAddressWithPrefix<NetworkChecked>>,
    mode: WalletValidationMode,
) -> Result<(), BridgeCliError> {
    let should_check_wallet = matches!(
        mode,
        WalletValidationMode::Label | WalletValidationMode::Both
    );
    let should_check_address = matches!(
        mode,
        WalletValidationMode::Address | WalletValidationMode::Both
    );

    if should_check_wallet {
        let label = label.ok_or_else(|| {
            BridgeCliError::Eyre(eyre::eyre!("Wallet label is required for validation"))
        })?;
        if label_exists(label)? {
            return Err(BridgeCliError::LabelAlreadyExists(label.to_string()));
        }
    }

    if should_check_address {
        let address = address.ok_or_else(|| {
            BridgeCliError::Eyre(eyre::eyre!("Address is required for validation"))
        })?;
        if address_exists(address)? {
            return Err(BridgeCliError::AddressAlreadyExists(
                address.address_with_prefix(),
            ));
        }
    }

    Ok(())
}

/// Parse and validate an imported wallet file
pub(crate) fn parse_and_validate_imported_wallet(
    file_path: &Path,
    label: Option<&str>,
) -> Result<WalletData, BridgeCliError> {
    use std::fs;

    if !file_path.exists() {
        return Err(BridgeCliError::WalletFileNotFound(
            file_path.display().to_string(),
        ));
    }

    if !file_path.is_file() {
        return Err(BridgeCliError::PathNotAFile(
            file_path.display().to_string(),
        ));
    }

    // Read and parse the wallet file
    let wallet_content = fs::read_to_string(file_path)?;
    let wallet_export: WalletExport = serde_json::from_str(&wallet_content).map_err(|e| {
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to parse wallet file '{}': {}",
            file_path.display(),
            e
        ))
    })?;

    let wallet_data: WalletData = wallet_export.try_into()?;

    let network = wallet_data.network;

    // Extract and validate required fields
    let wallet_address = TaprootAddressWithPrefix::from_string_with_prefix(
        &wallet_data.address.address_with_prefix(),
        network,
    )?;

    let label = if let Some(lbl) = label {
        lbl
    } else {
        &wallet_data.label
    };

    // Validate that both wallet label and address don't already exist
    validate_wallet_availability(
        Some(label),
        Some(&wallet_address),
        WalletValidationMode::Both,
    )?;

    // Check if encrypted data exists
    if wallet_data.encrypted_mnemonic.is_none() {
        return Err(BridgeCliError::MissingEncryptedMnemonicField);
    }

    if wallet_data.encrypted_private_key.is_none() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Missing encrypted private key field"
        )));
    }

    Ok(wallet_data)
}

pub(crate) fn ensure_wallet_exists<T>(
    address: &crate::structs::TaprootAddressWithPrefix<T>,
) -> Result<(), crate::errors::BridgeCliError>
where
    T: bitcoin::address::NetworkValidation,
    bitcoin::Address<T>: crate::structs::AddrDisplay,
{
    if !address_exists(address)? {
        return Err(crate::errors::BridgeCliError::WalletNotFound(
            address.address_without_prefix(),
        ));
    }
    Ok(())
}

/// Validate address purpose (reduces validation duplication)
pub fn validate_address_purpose<T>(
    address: &TaprootAddressWithPrefix<T>,
    expected_purpose: Purpose,
) -> Result<(), BridgeCliError>
where
    T: NetworkValidation,
{
    if address.purpose != expected_purpose {
        return Err(BridgeCliError::PurposeMismatch {
            expected: expected_purpose,
            found: address.purpose,
        });
    }
    Ok(())
}

/// Load a key with purpose validation and passphrase prompt
pub async fn load_key_with_purpose_check<T>(
    address: &TaprootAddressWithPrefix<T>,
    expected_purpose: Purpose,
) -> Result<SecureKeypair, BridgeCliError>
where
    T: NetworkValidation + Clone,
    bitcoin::Address<T>: AddrDisplay,
{
    validate_address_purpose(address, expected_purpose)?;
    let secure_passphrase = crate::wallet::passphrase::prompt_unlock_passphrase()?;
    load_key(address, &secure_passphrase).await
}
