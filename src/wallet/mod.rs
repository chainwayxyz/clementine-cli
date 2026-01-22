//! Wallet management and Bitcoin operations for Clementine CLI.
//!
//! This module provides comprehensive wallet functionality:
//! - Creating new encrypted wallets with generated mnemonics
//! - Importing wallets from mnemonics, private keys, or files
//! - Secure storage and retrieval of wallet data
//! - Backup and export operations
//!
//! ## Core Operations
//!
//! - **Wallet creation**: Generate new wallets with BIP-39 mnemonics
//! - **Import/export methods**: Support mnemonic, private key, and file imports, and wallet backup
//! - **Secure access**: All operations require passphrase authentication
//!

pub(crate) mod address;
pub(crate) mod encryption;
pub(crate) mod mnemonic;
pub(crate) mod passphrase;
pub(crate) mod wallet_storage;
pub(crate) mod wallet_utils;

pub type BitcoinAddress<V = bitcoin::address::NetworkChecked> = bitcoin::Address<V>;

pub use address::{
    AddrDisplay, Purpose, TaprootAddressWithPrefix, parse_address, parse_taproot_address,
    print_all_wallets_with_addresses, should_not_have_purpose,
};
pub use mnemonic::prompt_mnemonic;
pub use passphrase::{prompt_passphrase, prompt_unlock_passphrase};
pub use wallet_utils::{
    derive_and_validate_mnemonic_import, ensure_wallet_exists, load_key_with_purpose_check,
    validate_imported_wallet_file, validate_wallet_availability,
};

use bip39::Mnemonic;
use bitcoin::Network;
use bitcoin::address::NetworkChecked;
use bitcoin::address::NetworkUnchecked;
use bitcoin::address::NetworkValidation;
use eyre::eyre;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::btc::utils::SECP;
use crate::core::errors::BridgeCliError;
use crate::core::secure_types::SecureByteVec;
use crate::core::secure_types::{SecureKeypair, SecureSecretKey, SecureString};
use crate::sqlite_db::sqlite_client::SqliteDb;
use crate::wallet::address::calculate_taproot_address;
use crate::wallet::encryption::{aes_decrypt_secure, aes_encrypt_secure};
use crate::wallet::mnemonic::MNEMONIC_WORD_COUNT;
use crate::wallet::mnemonic::derive_private_key_from_mnemonic;
use crate::wallet::mnemonic::generate_mnemonic;
use crate::wallet::mnemonic::load_mnemonic;
use crate::wallet::wallet_storage::extract_wallet_data_to_file;
use crate::wallet::wallet_utils::load_key;
use crate::wallet::wallet_utils::{
    parse_and_validate_imported_wallet, validate_mnemonic_import, validate_private_key_import,
};
use bitcoin::secp256k1::{Keypair, SecretKey};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ImportMethod {
    #[serde(rename = "mnemonic_import")]
    Mnemonic,
    #[serde(rename = "file_import")]
    File,
    #[serde(rename = "private_key_import")]
    PrivateKey,
}

impl ImportMethod {
    pub fn as_str(&self) -> &str {
        match self {
            ImportMethod::Mnemonic => "mnemonic_import",
            ImportMethod::File => "file_import",
            ImportMethod::PrivateKey => "private_key_import",
        }
    }
}

impl fmt::Display for ImportMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ImportMethod {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mnemonic_import" => Ok(ImportMethod::Mnemonic),
            "file_import" => Ok(ImportMethod::File),
            "private_key_import" => Ok(ImportMethod::PrivateKey),
            _ => Err("invalid import method"),
        }
    }
}

pub async fn create_encrypted_wallet(
    network: Network,
    label: String,
    purpose: Purpose,
    passphrase: SecureString,
    sqlite_client: Option<&SqliteDb>,
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
    validate_wallet_availability(Some(&label), Some(&address), sqlite_client).await?;

    // Encrypt mnemonic and private key separately with different nonces
    let master_private_key_secure = derive_private_key_from_mnemonic(&mnemonic)?;

    let mnemonic_secure: SecureString = SecureString::init_with(|| mnemonic.to_string());

    let encrypted_mnemonic = aes_encrypt_secure(&mnemonic_secure, &passphrase)?;
    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)?;

    // Store encrypted wallet with separate encrypted fields
    wallet_storage::store_wallet_data(
        &address,
        network,
        Some(&encrypted_mnemonic),
        &encrypted_private_key,
        false,
        None,
        None,
        &label,
        sqlite_client,
    )
    .await?;

    Ok((address, mnemonic))
}

/// Backup a wallet file to a specified destination
pub async fn backup_wallet(
    address: &TaprootAddressWithPrefix<NetworkUnchecked>,
    destination_path: &Path,
    sqlite_client: Option<&SqliteDb>,
) -> Result<PathBuf, BridgeCliError> {
    ensure_wallet_exists(address, sqlite_client).await?;

    let final_dest = extract_wallet_data_to_file(address, destination_path, sqlite_client).await?;

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
pub async fn import_wallet_from_mnemonic(
    network: Network,
    label: &str,
    purpose: Purpose,
    mnemonic: Mnemonic,
    passphrase: SecureString,
    sqlite_client: Option<&SqliteDb>,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    let address = derive_and_validate_mnemonic_import(
        network,
        Some(label),
        purpose,
        &mnemonic,
        sqlite_client,
    )
    .await?;

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
        Some(&encrypted_mnemonic),
        &encrypted_private_key,
        true,
        Some(ImportMethod::Mnemonic),
        Some(ImportMethod::Mnemonic),
        label,
        sqlite_client,
    )
    .await?;

    Ok(address)
}

/// Import a wallet from a file path
pub async fn import_wallet_from_file(
    file_path: &Path,
    label: Option<&str>,
    passphrase: SecureString,
    sqlite_client: Option<&SqliteDb>,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Parse and validate the wallet file using helper function
    let wallet_data = parse_and_validate_imported_wallet(file_path, label, sqlite_client).await?;

    let import_method = wallet_data.effective_import_method();

    let is_private_key_import = matches!(import_method, Some(ImportMethod::PrivateKey));

    if let Some(encrypted_mnemonic_hex) = &wallet_data.encrypted_mnemonic {
        let encrypted_data =
            encryption::encrypted_data_from_hex(encrypted_mnemonic_hex).map_err(|e| {
                tracing::error!("Failed to parse encrypted mnemonic: {}", e);
                BridgeCliError::Eyre(eyre!("Failed to parse encrypted mnemonic"))
            })?;

        // Try to decrypt mnemonic to verify passphrase; map auth failure to incorrect passphrase
        match aes_decrypt_secure(&encrypted_data, &passphrase) {
            Ok(decrypted_mnemonic) => {
                // Additional validation: check if decrypted content looks like a valid mnemonic
                let mnemonic_str = decrypted_mnemonic.expose_secret();

                // Basic validation: should have words separated by spaces
                let word_count = mnemonic_str.split_whitespace().count();
                if word_count != MNEMONIC_WORD_COUNT {
                    tracing::error!(
                        "Decrypted mnemonic has invalid word count: expected {}, got {}",
                        MNEMONIC_WORD_COUNT,
                        word_count
                    );
                    return Err(BridgeCliError::MnemonicParseError);
                }

                validate_mnemonic_import(&decrypted_mnemonic, &wallet_data)?;
            }
            Err(BridgeCliError::DecryptionError) => {
                tracing::warn!(
                    "Failed to decrypt mnemonic during import: authentication failed (wrong passphrase or corrupted data)",
                );
                return Err(BridgeCliError::IncorrectPassphrase);
            }
            Err(e) => {
                tracing::error!("Failed to decrypt mnemonic during import: {}", e);
                return Err(e);
            }
        }
    } else if is_private_key_import {
        validate_private_key_import(&wallet_data, &passphrase, &wallet_data.address.address)?;
    } else {
        return Err(BridgeCliError::MissingEncryptedMnemonicField);
    }

    // Convert encrypted data from the original wallet
    let encrypted_mnemonic_data = if let Some(hex) = wallet_data.encrypted_mnemonic.as_ref() {
        Some(encryption::encrypted_data_from_hex(hex).map_err(|e| {
            tracing::error!("Failed to convert encrypted mnemonic: {}", e);
            BridgeCliError::Eyre(eyre!("Failed to convert encrypted mnemonic"))
        })?)
    } else {
        None
    };

    let encrypted_private_key_data =
        encryption::encrypted_data_from_hex(&wallet_data.encrypted_private_key).map_err(|e| {
            tracing::error!("Failed to convert encrypted private key: {}", e);
            BridgeCliError::Eyre(eyre!("Failed to convert encrypted private key"))
        })?;

    let network = wallet_data.network;

    let wallet_address = TaprootAddressWithPrefix::from_string_with_prefix(
        &wallet_data.address.address_with_prefix(),
        network,
    )?;

    let label = if let Some(lbl) = label {
        lbl
    } else {
        &wallet_data.label
    };

    let original_import_method = wallet_data.effective_import_method().cloned();

    // Use store_wallet_data function for consistent storage
    wallet_storage::store_wallet_data(
        &wallet_address,
        network,
        encrypted_mnemonic_data.as_ref(),
        &encrypted_private_key_data,
        true,
        original_import_method,
        Some(ImportMethod::File),
        label,
        sqlite_client,
    )
    .await?;

    Ok(wallet_address)
}

/// Import a wallet from a private key
pub async fn import_wallet_from_private_key(
    network: Network,
    label: &str,
    purpose: Purpose,
    private_key: SecureString,
    passphrase: SecureString,
    sqlite_client: Option<&SqliteDb>,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    let private_key_bytes = SecureByteVec::new(Box::new(
        hex::decode(private_key.expose_secret()).map_err(|e| {
            tracing::error!("Invalid private key hex format: {}", e);
            BridgeCliError::Eyre(eyre!("Invalid private key hex format"))
        })?,
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

    let master_private_key_secure = SecureString::init_with(|| {
        master_private_key
            .as_ref_inner()
            .display_secret()
            .to_string()
    });

    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .map_err(|e| {
            tracing::error!("Error encrypting private key: {}", e);
            BridgeCliError::PrivateKeyEncryptionFailed
        })?;

    wallet_storage::store_wallet_data(
        &address,
        network,
        None,
        &encrypted_private_key,
        true,
        Some(ImportMethod::PrivateKey),
        Some(ImportMethod::PrivateKey),
        label,
        sqlite_client,
    )
    .await?;

    Ok(address)
}

pub async fn get_mnemonic_from_wallet<T>(
    address: &TaprootAddressWithPrefix<T>,
    passphrase: &SecureString,
    sqlite_client: Option<&SqliteDb>,
) -> Result<Mnemonic, BridgeCliError>
where
    T: NetworkValidation + Clone,
    bitcoin::Address<T>: AddrDisplay,
{
    let mnemonic = load_mnemonic(address, passphrase, sqlite_client).await?;

    Ok(mnemonic)
}

pub async fn get_private_key_from_wallet<T>(
    address: &TaprootAddressWithPrefix<T>,
    passphrase: &SecureString,
    sqlite_client: Option<&SqliteDb>,
) -> Result<SecureSecretKey, BridgeCliError>
where
    T: NetworkValidation + Clone,
    bitcoin::Address<T>: AddrDisplay,
{
    let keypair = load_key(address, passphrase, sqlite_client).await?;

    Ok(keypair.secret_key())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite_db::test_utils::fresh_db_with_test_name;
    use crate::sqlite_db::wallet_db::{WalletExport, WalletTable};
    use crate::wallet::address::generate_address_from_mnemonic;
    use crate::wallet::encryption::{aes_encrypt_secure, encrypted_data_to_hex};
    use crate::wallet::wallet_storage;
    use bip39::Language;
    use rand::TryRngCore;
    use rand::distr::Alphanumeric;
    use rand::{Rng, rngs::OsRng};
    use tempfile::tempdir;

    fn sample_passphrase() -> SecureString {
        let rng = rand::rng();
        let pass: String = rng
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();
        SecureString::init_with(|| pass)
    }

    fn other_passphrase() -> SecureString {
        let rng = rand::rng();
        let mut pass: String = rng
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();

        // Ensure this differs from the common sample passphrase shape.
        pass.push('!');

        SecureString::init_with(|| pass)
    }

    fn sample_mnemonic() -> Mnemonic {
        Mnemonic::generate_in(Language::English, MNEMONIC_WORD_COUNT).unwrap()
    }

    fn alt_mnemonic() -> Mnemonic {
        Mnemonic::parse("agent agent agent agent agent agent agent agent agent agent agent agent")
            .unwrap()
    }

    fn sample_private_key_hex() -> SecureString {
        let mut bytes = [0u8; 32];
        OsRng
            .try_fill_bytes(&mut bytes)
            .expect("Can not fail to generate random bytes");
        let hex = hex::encode(bytes);
        SecureString::init_with(|| hex)
    }

    fn short_private_key_hex() -> SecureString {
        let mut bytes = [0u8; 15];
        OsRng
            .try_fill_bytes(&mut bytes)
            .expect("Can not fail to generate random bytes");
        let hex = hex::encode(bytes);
        SecureString::init_with(|| hex)
    }

    async fn insert_wallet_from_mnemonic(
        db: &SqliteDb,
        label: &str,
        mnemonic: &Mnemonic,
        purpose: Purpose,
        passphrase: &SecureString,
        imported: bool,
        import_method: Option<ImportMethod>,
    ) -> TaprootAddressWithPrefix<NetworkChecked> {
        let network = Network::Testnet4;
        let address = generate_address_from_mnemonic(mnemonic, network, purpose).unwrap();
        let mnemonic_secure: SecureString = SecureString::init_with(|| mnemonic.to_string());
        let private_key = derive_private_key_from_mnemonic(mnemonic).unwrap();
        let encrypted_mnemonic = aes_encrypt_secure(&mnemonic_secure, passphrase).unwrap();
        let encrypted_private_key = aes_encrypt_secure(&private_key, passphrase).unwrap();

        wallet_storage::store_wallet_data(
            &address,
            network,
            Some(&encrypted_mnemonic),
            &encrypted_private_key,
            imported,
            import_method.clone(),
            import_method,
            label,
            Some(db),
        )
        .await
        .unwrap();

        address
    }

    #[tokio::test]
    async fn create_encrypted_wallet_stores_wallet() {
        let db = fresh_db_with_test_name().await;
        let passphrase = sample_passphrase();

        let (address, _) = create_encrypted_wallet(
            Network::Testnet4,
            "label_create".to_string(),
            Purpose::Deposit,
            passphrase,
            Some(&db),
        )
        .await
        .unwrap();

        let exists = WalletTable::label_exists(db.pool(), "label_create")
            .await
            .unwrap();

        assert!(exists);

        let fetched = WalletTable::get_wallet_by_address(db.pool(), address.clone())
            .await
            .unwrap();
        assert!(fetched.is_some());

        let wallet = fetched.unwrap();
        assert_eq!(wallet.label, "label_create");
        assert_eq!(wallet.network, Network::Testnet4);
        assert!(wallet.created_at.timestamp() > 0);
        assert!(!wallet.imported);
        assert_eq!(wallet.import_method, None);
        assert!(!wallet.encryption_method.is_empty());
    }

    #[tokio::test]
    async fn create_encrypted_wallet_rejects_duplicate_label() {
        let db = fresh_db_with_test_name().await;

        create_encrypted_wallet(
            Network::Testnet4,
            "dup_label".to_string(),
            Purpose::Withdrawal,
            sample_passphrase(),
            Some(&db),
        )
        .await
        .unwrap();

        let err = create_encrypted_wallet(
            Network::Testnet4,
            "dup_label".to_string(),
            Purpose::Withdrawal,
            sample_passphrase(),
            Some(&db),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BridgeCliError::LabelAlreadyExists(_)));
    }

    #[tokio::test]
    async fn create_encrypted_wallet_rejects_duplicate_address() {
        let db = fresh_db_with_test_name().await;
        let passphrase = sample_passphrase();

        let mnemonic = sample_mnemonic();

        // First insert a wallet using a deterministic mnemonic
        insert_wallet_from_mnemonic(
            &db,
            "dup_addr_one",
            &mnemonic,
            Purpose::Deposit,
            &passphrase,
            false,
            None,
        )
        .await;

        let address =
            generate_address_from_mnemonic(&mnemonic, Network::Testnet4, Purpose::Deposit).unwrap();

        let mnemonic_secure: SecureString = SecureString::init_with(|| mnemonic.to_string());
        let private_key = derive_private_key_from_mnemonic(&mnemonic).unwrap();
        let encrypted_mnemonic = aes_encrypt_secure(&mnemonic_secure, &passphrase).unwrap();
        let encrypted_private_key = aes_encrypt_secure(&private_key, &passphrase).unwrap();

        let err = wallet_storage::store_wallet_data(
            &address,
            Network::Testnet4,
            Some(&encrypted_mnemonic),
            &encrypted_private_key,
            false,
            None,
            None,
            "dup_addr_two",
            Some(&db),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BridgeCliError::AddressAlreadyExists(_)));
    }

    #[tokio::test]
    async fn import_wallet_from_mnemonic_succeeds() {
        let db = fresh_db_with_test_name().await;
        let mnemonic = sample_mnemonic();
        let passphrase = sample_passphrase();

        let address = import_wallet_from_mnemonic(
            Network::Testnet4,
            "import_label",
            Purpose::Deposit,
            mnemonic.clone(),
            passphrase,
            Some(&db),
        )
        .await
        .unwrap();

        let exists = WalletTable::address_exists(db.pool(), &address)
            .await
            .unwrap();
        assert!(exists);

        let wallet = WalletTable::get_wallet_by_address(db.pool(), address.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(wallet.label, "import_label");
        assert_eq!(wallet.import_method, Some(ImportMethod::Mnemonic));
        assert!(wallet.imported);
    }

    #[tokio::test]
    async fn import_wallet_from_mnemonic_rejects_duplicate_address() {
        let db = fresh_db_with_test_name().await;
        let mnemonic = sample_mnemonic();

        import_wallet_from_mnemonic(
            Network::Testnet4,
            "label_one",
            Purpose::Withdrawal,
            mnemonic.clone(),
            sample_passphrase(),
            Some(&db),
        )
        .await
        .unwrap();

        let err = import_wallet_from_mnemonic(
            Network::Testnet4,
            "label_two",
            Purpose::Withdrawal,
            mnemonic,
            sample_passphrase(),
            Some(&db),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BridgeCliError::AddressAlreadyExists(_)));
    }

    #[tokio::test]
    async fn import_wallet_from_mnemonic_rejects_bad_phrase() {
        let bad = Mnemonic::parse("abandon abandon abandon");
        assert!(bad.is_err());
    }

    #[tokio::test]
    async fn import_wallet_from_mnemonic_rejects_label_conflict() {
        let db = fresh_db_with_test_name().await;

        let mnemonic_one = sample_mnemonic();
        let mnemonic_two = alt_mnemonic();

        import_wallet_from_mnemonic(
            Network::Testnet4,
            "same_label",
            Purpose::Deposit,
            mnemonic_one,
            sample_passphrase(),
            Some(&db),
        )
        .await
        .unwrap();

        let err = import_wallet_from_mnemonic(
            Network::Testnet4,
            "same_label",
            Purpose::Deposit,
            mnemonic_two,
            sample_passphrase(),
            Some(&db),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BridgeCliError::LabelAlreadyExists(_)));
    }

    #[tokio::test]
    async fn backup_wallet_exports_file() {
        let db = fresh_db_with_test_name().await;
        let passphrase = sample_passphrase();
        let mnemonic = sample_mnemonic();

        let address = insert_wallet_from_mnemonic(
            &db,
            "backup_label",
            &mnemonic,
            Purpose::Deposit,
            &passphrase,
            false,
            None,
        )
        .await;

        let addr_unchecked: TaprootAddressWithPrefix<NetworkUnchecked> =
            TaprootAddressWithPrefix::from(&address);

        // TempDir note: destructor ignores deletion errors (possible leaks if cleanup fails). We close() at the
        // end to surface issues; if the test fails before close, the destructor will still attempt cleanup.
        let dir = tempdir().unwrap();
        let dest = backup_wallet(&addr_unchecked, dir.path(), Some(&db))
            .await
            .unwrap();

        assert!(dest.exists());
        let content = std::fs::read_to_string(&dest).unwrap();
        let export: WalletExport = serde_json::from_str(&content).unwrap();
        assert_eq!(export.label, "backup_label");
        assert_eq!(export.address, address.address_with_prefix());
        assert_eq!(export.network, Network::Testnet4.to_string());
        assert!(export.encrypted_mnemonic.is_some());
        assert!(!export.encrypted_private_key.ciphertext.is_empty());

        dir.close().expect("Failed to close and delete temp dir");
    }

    #[tokio::test]
    async fn backup_wallet_not_found() {
        let db = fresh_db_with_test_name().await;
        let addr = TaprootAddressWithPrefix::from_string_with_prefix_unchecked(
            "depbcrt1pnmrmugapastum8ztvgwcn8hvq2avmcwh2j4ssru7rtyygkpqq98q4wyd6s",
        )
        .unwrap();

        // TempDir note: destructor ignores deletion errors (possible leaks if cleanup fails). We close() at the
        // end to surface issues; if the test fails before close, the destructor will still attempt cleanup.
        let dir = tempdir().unwrap();
        let err = backup_wallet(&addr, dir.path(), Some(&db))
            .await
            .unwrap_err();

        dir.close().expect("Failed to close and delete temp dir");
        assert!(matches!(err, BridgeCliError::WalletNotFound(_)));
    }

    #[tokio::test]
    async fn import_wallet_from_file_succeeds() {
        let db = fresh_db_with_test_name().await;
        let passphrase = sample_passphrase();
        let mnemonic = sample_mnemonic();
        let network = Network::Testnet4;
        let address = generate_address_from_mnemonic(&mnemonic, network, Purpose::Deposit).unwrap();

        let mnemonic_secure: SecureString = SecureString::init_with(|| mnemonic.to_string());
        let private_key = derive_private_key_from_mnemonic(&mnemonic).unwrap();
        let encrypted_mnemonic = aes_encrypt_secure(&mnemonic_secure, &passphrase).unwrap();
        let encrypted_private_key = aes_encrypt_secure(&private_key, &passphrase).unwrap();

        let export = WalletExport {
            label: "file_label".to_string(),
            address: address.address_with_prefix(),
            network: network.to_string(),
            encrypted_mnemonic: Some(encrypted_data_to_hex(&encrypted_mnemonic)),
            encrypted_private_key: encrypted_data_to_hex(&encrypted_private_key),
            created_at: chrono::Utc::now().to_rfc3339(),
            encryption_method: "aes256_gcm_argon2id_secure".to_string(),
            imported: true,
            original_import_method: Some(ImportMethod::File),
            import_method: Some(ImportMethod::File),
        };

        // TempDir note: destructor ignores deletion errors (possible leaks if cleanup fails). We close() at the
        // end to surface issues; if the test fails before close, the destructor will still attempt cleanup.
        let dir = tempdir().unwrap();
        let path = dir.path().join("wallet_file.json");
        std::fs::write(&path, serde_json::to_string(&export).unwrap()).unwrap();

        let imported = import_wallet_from_file(
            &path,
            None,
            SecureString::init_with(|| passphrase.expose_secret().to_string()),
            Some(&db),
        )
        .await
        .unwrap();

        dir.close().expect("Failed to close and delete temp dir");

        assert_eq!(
            imported.address_with_prefix(),
            address.address_with_prefix()
        );

        let fetched = WalletTable::get_wallet_by_address(db.pool(), imported)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.import_method, Some(ImportMethod::File));
        assert!(fetched.imported);
    }

    #[tokio::test]
    async fn import_wallet_from_file_rejects_incorrect_passphrase() {
        let db = fresh_db_with_test_name().await;
        let passphrase = sample_passphrase();
        let mnemonic = sample_mnemonic();
        let network = Network::Testnet4;
        let address = generate_address_from_mnemonic(&mnemonic, network, Purpose::Deposit).unwrap();

        let mnemonic_secure: SecureString = SecureString::init_with(|| mnemonic.to_string());
        let private_key = derive_private_key_from_mnemonic(&mnemonic).unwrap();
        let encrypted_mnemonic = aes_encrypt_secure(&mnemonic_secure, &passphrase).unwrap();
        let encrypted_private_key = aes_encrypt_secure(&private_key, &passphrase).unwrap();

        let export = WalletExport {
            label: "file_label".to_string(),
            address: address.address_with_prefix(),
            network: network.to_string(),
            encrypted_mnemonic: Some(encrypted_data_to_hex(&encrypted_mnemonic)),
            encrypted_private_key: encrypted_data_to_hex(&encrypted_private_key),
            created_at: chrono::Utc::now().to_rfc3339(),
            encryption_method: "aes256_gcm_argon2id_secure".to_string(),
            imported: true,
            original_import_method: Some(ImportMethod::File),
            import_method: Some(ImportMethod::File),
        };

        // TempDir note: destructor ignores deletion errors (possible leaks if cleanup fails). We close() at the
        // end to surface issues; if the test fails before close, the destructor will still attempt cleanup.
        let dir = tempdir().unwrap();
        let path = dir.path().join("wallet_file.json");
        std::fs::write(&path, serde_json::to_string(&export).unwrap()).unwrap();

        let err = import_wallet_from_file(&path, None, other_passphrase(), Some(&db))
            .await
            .unwrap_err();

        dir.close().expect("Failed to close and delete temp dir");
        assert!(matches!(err, BridgeCliError::IncorrectPassphrase));
    }

    #[tokio::test]
    async fn import_wallet_from_file_rejects_missing_mnemonic_for_non_private_key() {
        let db = fresh_db_with_test_name().await;
        let passphrase = sample_passphrase();
        let mnemonic = sample_mnemonic();
        let network = Network::Testnet4;
        let address = generate_address_from_mnemonic(&mnemonic, network, Purpose::Deposit).unwrap();

        let private_key = derive_private_key_from_mnemonic(&mnemonic).unwrap();
        let encrypted_private_key = aes_encrypt_secure(&private_key, &passphrase).unwrap();

        let export = WalletExport {
            label: "missing_mnemonic_label".to_string(),
            address: address.address_with_prefix(),
            network: network.to_string(),
            encrypted_mnemonic: None,
            encrypted_private_key: encrypted_data_to_hex(&encrypted_private_key),
            created_at: chrono::Utc::now().to_rfc3339(),
            encryption_method: "aes256_gcm_argon2id_secure".to_string(),
            imported: true,
            original_import_method: Some(ImportMethod::Mnemonic),
            import_method: Some(ImportMethod::Mnemonic),
        };

        // TempDir note: destructor ignores deletion errors (possible leaks if cleanup fails). We close() at the
        // end to surface issues; if the test fails before close, the destructor will still attempt cleanup.
        let dir = tempdir().unwrap();
        let path = dir.path().join("wallet_missing_mnemonic.json");
        std::fs::write(&path, serde_json::to_string(&export).unwrap()).unwrap();

        let err = import_wallet_from_file(
            &path,
            None,
            SecureString::init_with(|| passphrase.expose_secret().to_string()),
            Some(&db),
        )
        .await
        .unwrap_err();

        dir.close().expect("Failed to close and delete temp dir");
        assert!(matches!(err, BridgeCliError::MissingEncryptedMnemonicField));
    }

    #[tokio::test]
    async fn import_wallet_from_private_key_succeeds() {
        let db = fresh_db_with_test_name().await;

        let address = import_wallet_from_private_key(
            Network::Testnet4,
            "pk_label",
            Purpose::Withdrawal,
            sample_private_key_hex(),
            sample_passphrase(),
            Some(&db),
        )
        .await
        .unwrap();

        let wallet = WalletTable::get_wallet_by_address(db.pool(), address.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(wallet.label, "pk_label");
        assert_eq!(wallet.import_method, Some(ImportMethod::PrivateKey));
        assert!(wallet.imported);
    }

    #[tokio::test]
    async fn import_wallet_from_private_key_rejects_invalid_length() {
        let db = fresh_db_with_test_name().await;
        let err = import_wallet_from_private_key(
            Network::Testnet4,
            "pk_bad",
            Purpose::Deposit,
            short_private_key_hex(),
            sample_passphrase(),
            Some(&db),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BridgeCliError::InvalidPrivateKey(_)));
    }

    #[tokio::test]
    async fn get_mnemonic_from_wallet_rejects_private_key_file_import() {
        let test_name = std::thread::current()
            .name()
            .expect("Failed to get current thread name for test database")
            .split(':')
            .next_back()
            .expect("Failed to get last segment of thread name")
            .to_string();
        let source_db = SqliteDb::open_in_memory_with_schema(&format!("{test_name}_source"))
            .await
            .expect("Failed to open in-memory test DB");
        let target_db = SqliteDb::open_in_memory_with_schema(&format!("{test_name}_target"))
            .await
            .expect("Failed to open in-memory test DB");
        let passphrase = sample_passphrase();
        let passphrase_for_import =
            SecureString::init_with(|| passphrase.expose_secret().to_string());
        let passphrase_for_show =
            SecureString::init_with(|| passphrase.expose_secret().to_string());

        let address = import_wallet_from_private_key(
            Network::Testnet4,
            "pk_file_source",
            Purpose::Deposit,
            sample_private_key_hex(),
            passphrase,
            Some(&source_db),
        )
        .await
        .unwrap();

        let addr_unchecked: TaprootAddressWithPrefix<NetworkUnchecked> =
            TaprootAddressWithPrefix::from(&address);

        // TempDir note: destructor ignores deletion errors (possible leaks if cleanup fails). We close() at the
        // end to surface issues; if the test fails before close, the destructor will still attempt cleanup.
        let dir = tempdir().unwrap();
        let path = backup_wallet(&addr_unchecked, dir.path(), Some(&source_db))
            .await
            .unwrap();

        let imported_address =
            import_wallet_from_file(&path, None, passphrase_for_import, Some(&target_db))
                .await
                .unwrap();

        dir.close().expect("Failed to close and delete temp dir");

        let wallet = WalletTable::get_wallet_by_address(target_db.pool(), imported_address.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(wallet.import_method, Some(ImportMethod::File));
        assert_eq!(
            wallet.original_import_method,
            Some(ImportMethod::PrivateKey)
        );

        let err =
            get_mnemonic_from_wallet(&imported_address, &passphrase_for_show, Some(&target_db))
                .await
                .unwrap_err();
        assert!(matches!(err, BridgeCliError::NoMnemonicAvailable));
    }

    #[tokio::test]
    async fn get_mnemonic_from_wallet_succeeds_after_mnemonic_file_import() {
        let test_name = std::thread::current()
            .name()
            .expect("Failed to get current thread name for test database")
            .split(':')
            .next_back()
            .expect("Failed to get last segment of thread name")
            .to_string();
        let source_db = SqliteDb::open_in_memory_with_schema(&format!("{test_name}_source"))
            .await
            .expect("Failed to open in-memory test DB");
        let target_db = SqliteDb::open_in_memory_with_schema(&format!("{test_name}_target"))
            .await
            .expect("Failed to open in-memory test DB");
        let passphrase = sample_passphrase();
        let passphrase_for_import =
            SecureString::init_with(|| passphrase.expose_secret().to_string());
        let passphrase_for_show =
            SecureString::init_with(|| passphrase.expose_secret().to_string());
        let mnemonic = sample_mnemonic();
        let expected_mnemonic = mnemonic.clone();

        let address = import_wallet_from_mnemonic(
            Network::Testnet4,
            "mnemonic_file_source",
            Purpose::Deposit,
            mnemonic,
            passphrase,
            Some(&source_db),
        )
        .await
        .unwrap();

        let addr_unchecked: TaprootAddressWithPrefix<NetworkUnchecked> =
            TaprootAddressWithPrefix::from(&address);

        // TempDir note: destructor ignores deletion errors (possible leaks if cleanup fails). We close() at the
        // end to surface issues; if the test fails before close, the destructor will still attempt cleanup.
        let dir = tempdir().unwrap();
        let path = backup_wallet(&addr_unchecked, dir.path(), Some(&source_db))
            .await
            .unwrap();

        let imported_address =
            import_wallet_from_file(&path, None, passphrase_for_import, Some(&target_db))
                .await
                .unwrap();

        dir.close().expect("Failed to close and delete temp dir");

        let wallet = WalletTable::get_wallet_by_address(target_db.pool(), imported_address.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(wallet.import_method, Some(ImportMethod::File));
        assert_eq!(wallet.original_import_method, Some(ImportMethod::Mnemonic));

        let fetched =
            get_mnemonic_from_wallet(&imported_address, &passphrase_for_show, Some(&target_db))
                .await
                .unwrap();
        assert_eq!(fetched.to_string(), expected_mnemonic.to_string());
    }

    #[tokio::test]
    async fn get_mnemonic_from_wallet_returns_plaintext() {
        let db = fresh_db_with_test_name().await;
        let mnemonic = sample_mnemonic();
        let passphrase = sample_passphrase();

        let address = import_wallet_from_mnemonic(
            Network::Testnet4,
            "mnemonic_fetch",
            Purpose::Deposit,
            mnemonic.clone(),
            SecureString::init_with(|| passphrase.expose_secret().to_string()),
            Some(&db),
        )
        .await
        .unwrap();

        let fetched = get_mnemonic_from_wallet(&address, &passphrase, Some(&db))
            .await
            .unwrap();
        assert_eq!(fetched.to_string(), mnemonic.to_string());
    }

    #[tokio::test]
    async fn get_mnemonic_from_wallet_without_mnemonic_returns_error() {
        let db = fresh_db_with_test_name().await;
        let passphrase = sample_passphrase();

        let address = import_wallet_from_private_key(
            Network::Testnet4,
            "no_mnemonic",
            Purpose::Deposit,
            sample_private_key_hex(),
            SecureString::init_with(|| passphrase.expose_secret().to_string()),
            Some(&db),
        )
        .await
        .unwrap();

        let err = get_mnemonic_from_wallet(&address, &passphrase, Some(&db))
            .await
            .unwrap_err();
        assert!(matches!(err, BridgeCliError::NoMnemonicAvailable));
    }

    #[tokio::test]
    async fn get_mnemonic_from_wallet_incorrect_passphrase() {
        let db = fresh_db_with_test_name().await;
        let mnemonic = sample_mnemonic();

        let address = import_wallet_from_mnemonic(
            Network::Testnet4,
            "mnemonic_wrong_pass",
            Purpose::Deposit,
            mnemonic,
            sample_passphrase(),
            Some(&db),
        )
        .await
        .unwrap();

        let err = get_mnemonic_from_wallet(&address, &other_passphrase(), Some(&db))
            .await
            .unwrap_err();
        assert!(matches!(err, BridgeCliError::IncorrectPassphrase));
    }

    #[tokio::test]
    async fn get_private_key_from_wallet_returns_key() {
        let db = fresh_db_with_test_name().await;
        let mnemonic = sample_mnemonic();
        let passphrase = sample_passphrase();

        let address = import_wallet_from_mnemonic(
            Network::Testnet4,
            "pk_fetch",
            Purpose::Deposit,
            mnemonic.clone(),
            SecureString::init_with(|| passphrase.expose_secret().to_string()),
            Some(&db),
        )
        .await
        .unwrap();

        let key = get_private_key_from_wallet(&address, &passphrase, Some(&db))
            .await
            .unwrap();

        let expected = derive_private_key_from_mnemonic(&mnemonic).unwrap();
        assert_eq!(
            key.as_ref_inner().display_secret().to_string(),
            expected.expose_secret().to_string()
        );
    }

    #[tokio::test]
    async fn get_private_key_from_wallet_incorrect_passphrase() {
        let db = fresh_db_with_test_name().await;
        let mnemonic = sample_mnemonic();

        let address = import_wallet_from_mnemonic(
            Network::Testnet4,
            "pk_wrong_pass",
            Purpose::Deposit,
            mnemonic,
            sample_passphrase(),
            Some(&db),
        )
        .await
        .unwrap();

        let err = get_private_key_from_wallet(&address, &other_passphrase(), Some(&db)).await;
        assert!(matches!(err, Err(BridgeCliError::IncorrectPassphrase)));
    }

    #[tokio::test]
    async fn get_private_key_from_wallet_not_found() {
        let db = fresh_db_with_test_name().await;
        let addr = TaprootAddressWithPrefix::from_string_with_prefix(
            "depbcrt1pnmrmugapastum8ztvgwcn8hvq2avmcwh2j4ssru7rtyygkpqq98q4wyd6s",
            Network::Regtest,
        )
        .unwrap();

        let err = get_private_key_from_wallet(&addr, &sample_passphrase(), Some(&db)).await;
        assert!(matches!(err, Err(BridgeCliError::WalletNotFound(_))));
    }
}
