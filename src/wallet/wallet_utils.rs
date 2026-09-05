//! Wallet validation, key management, and utility functions for Clementine CLI.
//!
//! This module provides essential wallet operations and validation:
//! - Loading and validating encrypted keys from storage
//! - Network parsing and address validation
//! - Wallet existence and availability checking
//! - Import validation for mnemonics and private keys
//!
//! ## Validation Features
//!
//! - **Wallet availability**: Checks for label and address conflicts
//! - **Import validation**: Verifies mnemonic and private key imports
//! - **Address derivation**: Confirms imported data matches addresses
//!

use crate::core::errors::BridgeCliError;
use crate::core::secure_types::SecureKeypair;
use crate::core::secure_types::SecureSecretKey;
use crate::core::secure_types::SecureString;
use crate::sqlite_db::sqlite_client::{SqliteDb, resolve_sqlite_client};
use crate::sqlite_db::wallet_db::WalletExport;
use crate::sqlite_db::wallet_db::{WalletData, WalletTable};
use crate::wallet::AddrDisplay;
use crate::wallet::Purpose;
use crate::wallet::TaprootAddressWithPrefix;
use crate::wallet::address::calculate_taproot_address;
use crate::wallet::address::generate_address_from_mnemonic;
use crate::wallet::encryption::aes_decrypt_secure;
use crate::wallet::wallet_storage::load_wallet_data;
use bip39::Mnemonic;
use bitcoin::Address;
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

/// Load and decrypt a wallet's private key.
pub(crate) async fn load_key<T>(
    address: &TaprootAddressWithPrefix<T>,
    passphrase: &SecureString,
    sqlite_client: Option<&SqliteDb>,
) -> Result<SecureKeypair, BridgeCliError>
where
    T: NetworkValidation + Clone,
    bitcoin::Address<T>: AddrDisplay,
{
    let wallet_data = load_wallet_data(address, sqlite_client)
        .await?
        .ok_or_else(|| BridgeCliError::WalletNotFound(address.address_with_prefix()))?;

    let encrypted_private_data =
        crate::wallet::encryption::encrypted_data_from_hex(&wallet_data.encrypted_private_key)
            .map_err(|e| {
                tracing::error!("Failed to parse encrypted private key: {}", e);
                BridgeCliError::Eyre(eyre!("Failed to parse encrypted private key"))
            })?;

    let decrypted_key = match aes_decrypt_secure(&encrypted_private_data, passphrase) {
        Ok(key) => key,
        Err(BridgeCliError::DecryptionError) => {
            tracing::warn!(
                "Failed to decrypt private key: authentication failed (wrong passphrase or corrupted data)",
            );
            return Err(BridgeCliError::IncorrectPassphrase);
        }
        Err(e) => return Err(e),
    };

    let secp = Secp256k1::new();
    let secret_key = SecureSecretKey::new(SecretKey::from_str(decrypted_key.expose_secret())?);
    let keypair = Keypair::from_secret_key(&secp, secret_key.as_ref_inner());

    Ok(SecureKeypair::new(keypair))
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
    wallet_address: &Address,
) -> Result<(), BridgeCliError> {
    let encrypted_private_data =
        crate::wallet::encryption::encrypted_data_from_hex(&wallet_data.encrypted_private_key)
            .map_err(|e| {
                tracing::error!("Failed to parse encrypted private key: {}", e);
                BridgeCliError::Eyre(eyre!("Failed to parse encrypted private key"))
            })?;

    // Decrypt and validate the private key
    match aes_decrypt_secure(&encrypted_private_data, passphrase) {
        Ok(decrypted_private_key) => {
            let network = parse_network(&wallet_data.network.to_string())?;

            // Validate the private key format and derive address to verify
            match SecretKey::from_str(decrypted_private_key.expose_secret()) {
                Ok(private_key) => {
                    let secure_secret_key = SecureSecretKey::new(private_key);
                    let keypair = SecureKeypair::new(Keypair::from_secret_key(
                        &crate::btc::utils::SECP,
                        secure_secret_key.as_ref_inner(),
                    ));
                    let derived_address = calculate_taproot_address(&keypair, network);

                    if &derived_address != wallet_address {
                        return Err(BridgeCliError::AddressMismatch);
                    }
                }
                Err(e) => {
                    tracing::error!("Invalid private key format: {}", e);
                    return Err(BridgeCliError::InvalidPrivateKey(
                        "Invalid private key format".to_string(),
                    ));
                }
            }
        }
        Err(BridgeCliError::DecryptionError) => {
            tracing::warn!(
                "Failed to decrypt private key: authentication failed (wrong passphrase or corrupted data)"
            );
            return Err(BridgeCliError::IncorrectPassphrase);
        }
        Err(e) => {
            tracing::error!("Failed to decrypt private key: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

pub(crate) async fn label_exists(
    label: &str,
    sqlite_client: Option<&SqliteDb>,
) -> Result<bool, BridgeCliError> {
    let sqlite_client = resolve_sqlite_client(sqlite_client).await?;
    WalletTable::label_exists(sqlite_client.as_ref().pool(), label).await
}

pub(crate) async fn address_exists<T>(
    address: &TaprootAddressWithPrefix<T>,
    sqlite_client: Option<&SqliteDb>,
) -> Result<bool, BridgeCliError>
where
    T: bitcoin::address::NetworkValidation,
    bitcoin::Address<T>: AddrDisplay,
{
    let sqlite_client = resolve_sqlite_client(sqlite_client).await?;
    WalletTable::address_exists(sqlite_client.as_ref().pool(), address).await
}

/// Combined validation function to check for conflicts during wallet operations
pub async fn validate_wallet_availability(
    label: Option<&str>,
    address: Option<&TaprootAddressWithPrefix<NetworkChecked>>,
    sqlite_client: Option<&SqliteDb>,
) -> Result<(), BridgeCliError> {
    if let Some(label) = label
        && label_exists(label, sqlite_client).await?
    {
        return Err(BridgeCliError::LabelAlreadyExists(label.to_string()));
    }

    if let Some(address) = address
        && address_exists(address, sqlite_client).await?
    {
        return Err(BridgeCliError::AddressAlreadyExists(
            address.address_with_prefix(),
        ));
    }

    Ok(())
}

/// Derive address from mnemonic and ensure both label and address are available
pub async fn derive_and_validate_mnemonic_import(
    network: Network,
    label: Option<&str>,
    purpose: Purpose,
    mnemonic: &Mnemonic,
    sqlite_client: Option<&SqliteDb>,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    let address = generate_address_from_mnemonic(mnemonic, network, purpose).map_err(|e| {
        tracing::error!("Error generating address from mnemonic: {}", e);
        BridgeCliError::AddressGenerationFromMnemonicFailed
    })?;

    validate_wallet_availability(label, Some(&address), sqlite_client).await?;

    Ok(address)
}

/// Parse and validate an imported wallet file
pub(crate) async fn parse_and_validate_imported_wallet(
    file_path: &Path,
    label: Option<&str>,
    sqlite_client: Option<&SqliteDb>,
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
        tracing::error!(
            "Failed to parse wallet file '{}': {}",
            file_path.display(),
            e
        );
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to parse wallet file '{}'",
            file_path.display()
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
    validate_wallet_availability(Some(label), Some(&wallet_address), sqlite_client).await?;

    let import_method = wallet_data.effective_import_method();
    let is_private_key_import =
        matches!(import_method, Some(crate::wallet::ImportMethod::PrivateKey));

    // Check if encrypted data exists
    if wallet_data.encrypted_mnemonic.is_none() && !is_private_key_import {
        return Err(BridgeCliError::MissingEncryptedMnemonicField);
    }

    Ok(wallet_data)
}

pub async fn validate_imported_wallet_file(
    file_path: &Path,
    label: Option<&str>,
    sqlite_client: Option<&SqliteDb>,
) -> Result<(), BridgeCliError> {
    parse_and_validate_imported_wallet(file_path, label, sqlite_client)
        .await
        .map(|_| ())
}

pub async fn ensure_wallet_exists<T>(
    address: &TaprootAddressWithPrefix<T>,
    sqlite_client: Option<&SqliteDb>,
) -> Result<(), BridgeCliError>
where
    T: bitcoin::address::NetworkValidation,
    bitcoin::Address<T>: AddrDisplay,
{
    if !address_exists(address, sqlite_client).await? {
        return Err(BridgeCliError::WalletNotFound(
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
    tracing::debug!("Validating purpose for address {:?}", address.address);
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
    sqlite_client: Option<&SqliteDb>,
) -> Result<SecureKeypair, BridgeCliError>
where
    T: NetworkValidation + Clone,
    bitcoin::Address<T>: AddrDisplay,
{
    validate_address_purpose(address, expected_purpose)?;
    let secure_passphrase = crate::wallet::passphrase::prompt_unlock_passphrase()?;
    tracing::debug!("Loading key for address {}", address.address_with_prefix());
    load_key(address, &secure_passphrase, sqlite_client).await
}

/// Check if a Bitcoin address is a withdrawal wallet address
pub(crate) async fn is_withdrawal_address_wallet_address(
    address: &crate::wallet::BitcoinAddress,
    config: &crate::core::config::BridgeCliConfig,
    sqlite_client: Option<&SqliteDb>,
) -> Result<bool, BridgeCliError> {
    if address.address_type() == Some(bitcoin::AddressType::P2tr) {
        let address = TaprootAddressWithPrefix::from_string_without_prefix(
            &address.to_string(),
            Purpose::Withdrawal,
            config.network,
        )?;
        return address_exists(&address, sqlite_client).await;
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite_db::test_utils::fresh_db_with_test_name;
    use crate::sqlite_db::wallet_db::WalletExport;
    use crate::wallet::ImportMethod;
    use crate::wallet::address::generate_address_from_mnemonic;
    use crate::wallet::encryption::{aes_encrypt_secure, encrypted_data_to_hex};
    use crate::wallet::mnemonic::MNEMONIC_WORD_COUNT;
    use bip39::Language;
    use bip39::Mnemonic;
    use tempfile::tempdir;

    fn sample_encrypted_data() -> crate::wallet::encryption::EncryptedDataHex {
        let plaintext = SecureString::init_with(|| "dummy secret".to_string());
        let passphrase = SecureString::init_with(|| "passphrase".to_string());
        let encrypted = aes_encrypt_secure(&plaintext, &passphrase).expect("encrypt");
        encrypted_data_to_hex(&encrypted)
    }

    fn sample_wallet_export(
        import_method: ImportMethod,
        include_mnemonic: bool,
    ) -> (
        WalletExport,
        TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    ) {
        let network = Network::Testnet4;
        let mnemonic =
            Mnemonic::generate_in(Language::English, MNEMONIC_WORD_COUNT).expect("mnemonic");
        let address =
            generate_address_from_mnemonic(&mnemonic, network, Purpose::Deposit).expect("address");

        let encrypted_private_key = sample_encrypted_data();
        let encrypted_mnemonic = if include_mnemonic {
            Some(sample_encrypted_data())
        } else {
            None
        };

        let export = WalletExport {
            label: "test-wallet".to_string(),
            address: address.address_with_prefix(),
            network: network.to_string(),
            encrypted_mnemonic,
            encrypted_private_key,
            created_at: chrono::Utc::now().to_rfc3339(),
            encryption_method: "aes256_gcm_argon2id_secure".to_string(),
            imported: true,
            original_import_method: Some(import_method.clone()),
            import_method: Some(import_method),
        };

        (export, address)
    }

    #[test]
    fn parse_network_accepts_expected_values() {
        assert_eq!(
            parse_network("testnet4").expect("testnet4"),
            Network::Testnet4
        );
        assert_eq!(parse_network("regtest").expect("regtest"), Network::Regtest);
        assert_eq!(parse_network("signet").expect("signet"), Network::Signet);
        assert_eq!(parse_network("bitcoin").expect("bitcoin"), Network::Bitcoin);
    }

    #[test]
    fn parse_network_rejects_testnet() {
        let err = parse_network("testnet").expect_err("unsupported network");
        assert!(matches!(
            err,
            BridgeCliError::UnsupportedNetwork(Network::Testnet)
        ));
    }

    #[tokio::test]
    async fn parse_and_validate_imported_wallet_allows_private_key_without_mnemonic() {
        let db = fresh_db_with_test_name().await;
        let (export, address) = sample_wallet_export(ImportMethod::PrivateKey, false);
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("wallet.json");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&export).expect("serialize"),
        )
        .expect("write");

        let wallet_data = parse_and_validate_imported_wallet(&path, None, Some(&db))
            .await
            .expect("parse");

        assert_eq!(
            wallet_data.address.address_with_prefix(),
            address.address_with_prefix()
        );
        assert!(wallet_data.encrypted_mnemonic.is_none());
        dir.close().expect("close tempdir");
    }

    #[tokio::test]
    async fn parse_and_validate_imported_wallet_requires_mnemonic_for_non_private_key() {
        let db = fresh_db_with_test_name().await;
        let (export, _address) = sample_wallet_export(ImportMethod::Mnemonic, false);
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("wallet.json");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&export).expect("serialize"),
        )
        .expect("write");

        let err = parse_and_validate_imported_wallet(&path, None, Some(&db))
            .await
            .expect_err("error");
        assert!(matches!(err, BridgeCliError::MissingEncryptedMnemonicField));
        dir.close().expect("close tempdir");
    }
}
