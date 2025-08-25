use crate::BitcoinAddress;
use crate::bitcoin_utils::calculate_taproot_address;
use crate::errors::BridgeCliError;
use crate::structs::SecureKeypair;
use crate::structs::SecureSecretKey;
use crate::structs::SecureString;
use crate::wallet::address::generate_address_from_mnemonic_secure;
use crate::wallet::address::parse_address;
use crate::wallet::encryption::aes_decrypt_secure;
use crate::wallet::wallet_storage::GenericWalletData;
use crate::wallet::wallet_storage::get_storage_dir;
use crate::wallet::wallet_storage::get_wallets_from_registry;
use crate::wallet::wallet_storage::load_wallet_data;
use bitcoin::Network;
use bitcoin::key::Keypair;
use bitcoin::secp256k1::Secp256k1;
use bitcoin::secp256k1::SecretKey;
use eyre::eyre;
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::str::FromStr;

/// Securely load a key from wallet storage
pub(crate) fn load_key(
    wallet_name: &str,
    passphrase: &SecureString,
) -> Result<SecureKeypair, BridgeCliError> {
    if !wallet_exists(wallet_name)? {
        return Err(BridgeCliError::WalletNotFound(wallet_name.to_string()));
    }

    // Load wallet data
    let wallet_data = load_wallet_data(wallet_name)?;

    // Load the encrypted private key
    let encrypted_private_key = wallet_data
        .encrypted_private_key
        .ok_or_else(|| BridgeCliError::NoEncryptedPrivateKeyFound)?;

    let encrypted_data =
        crate::wallet::encryption::encrypted_data_from_hex(&encrypted_private_key)?;
    let decrypted_key = aes_decrypt_secure(&encrypted_data, passphrase)?;

    let secp = Secp256k1::new();
    let secret_key = SecureSecretKey::new(SecretKey::from_str(decrypted_key.expose_secret())?);

    let keypair = Keypair::from_secret_key(&secp, secret_key.as_ref());
    let secure_keypair = SecureKeypair::new(keypair);

    Ok(secure_keypair)
}

/// Helper function to validate mnemonic imports during wallet import
pub(crate) fn validate_mnemonic_import(
    decrypted_mnemonic: &SecureString,
    wallet_data: &GenericWalletData,
    wallet_address: &str,
) -> Result<(), BridgeCliError> {
    let network = parse_network(&wallet_data.network)?;

    // Generate address from mnemonic to verify it matches
    match generate_address_from_mnemonic_secure(decrypted_mnemonic, network) {
        Ok(derived_address) => {
            if derived_address != wallet_address {
                return Err(BridgeCliError::AddressMismatch);
            }
            println!("Passphrase verified successfully! Address confirmed.");
        }
        Err(e) => return Err(BridgeCliError::MnemonicParseError(e.to_string())),
    }

    Ok(())
}

fn check_network_compatibility(
    wallet_network: Network,
    network: Network,
) -> Result<(), BridgeCliError> {
    if wallet_network != network {
        return Err(BridgeCliError::NetworkMismatch(
            wallet_network.to_string(),
            network.to_string(),
        ));
    }

    Ok(())
}

/// Helper function to parse network string into Network enum
pub(crate) fn parse_network(network_str: &str) -> Result<Network, BridgeCliError> {
    match network_str {
        "testnet4" => Ok(Network::Testnet4),
        "testnet" => Ok(Network::Testnet),
        "regtest" => Ok(Network::Regtest),
        "signet" => Ok(Network::Signet),
        "bitcoin" => Ok(Network::Bitcoin),
        _ => Err(BridgeCliError::UnsupportedNetwork),
    }
}

/// Helper function to validate private key imports during wallet import
pub(crate) fn validate_private_key_import(
    wallet_data: &GenericWalletData,
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
                let network = parse_network(&wallet_data.network)?;

                // Validate the private key format and derive address to verify
                match SecretKey::from_str(decrypted_private_key.expose_secret()) {
                    Ok(private_key) => {
                        let secure_secret_key = SecureSecretKey::new(private_key);
                        let keypair = SecureKeypair::new(Keypair::from_secret_key(
                            &crate::bitcoin_utils::SECP,
                            secure_secret_key.as_ref(),
                        ));
                        let derived_address = calculate_taproot_address(&keypair, network);

                        if derived_address.to_string() != wallet_address {
                            return Err(BridgeCliError::AddressMismatch);
                        }
                        println!(
                            "Passphrase verified successfully! Private key address confirmed."
                        );
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

pub(crate) fn wallet_exists(wallet_name: &str) -> Result<bool, BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_name));
    Ok(wallet_file.exists())
}

pub(crate) fn address_exists(address: &str, network: Network) -> Result<bool, BridgeCliError> {
    // Use the registry to check all wallets efficiently
    let wallets = get_wallets_from_registry()?;

    for (_wallet_name, wallet_entry) in wallets {
        if wallet_entry.address == address {
            let wallet_network = parse_network(&wallet_entry.network)?;
            if wallet_network == network {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

pub(crate) fn load_address_from_registry(
    wallet_name: &str,
    network: Network,
) -> Result<BitcoinAddress, BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    let wallets_file = storage_dir.join("wallets.json");

    if !wallets_file.exists() {
        return Err(BridgeCliError::WalletsRegistryNotFound);
    }

    let wallets_content = std::fs::read_to_string(&wallets_file)?;
    let wallets: HashMap<String, serde_json::Value> = serde_json::from_str(&wallets_content)?;

    if let Some(wallet_data) = wallets.get(wallet_name)
        && let Some(address_str) = wallet_data.get("address").and_then(|a| a.as_str())
        && let Some(network_str) = wallet_data.get("network").and_then(|n| n.as_str())
    {
        let wallet_network = parse_network(network_str)?;
        check_network_compatibility(wallet_network, network)?;
        return parse_address(address_str, network);
    }

    Err(BridgeCliError::WalletNotFound(wallet_name.to_string()))
}

/// Validation options for wallet creation and import operations
#[derive(Debug)]
pub enum WalletValidationMode {
    /// Check if wallet name already exists
    WalletName,
    /// Check if address already exists for the given network
    Address,
    /// Check both wallet name and address
    Both,
}

/// Combined validation function to check for conflicts during wallet operations
pub(crate) fn validate_wallet_availability(
    wallet_name: Option<&str>,
    address: Option<&str>,
    network: Option<Network>,
    mode: WalletValidationMode,
) -> Result<(), BridgeCliError> {
    let should_check_wallet = matches!(
        mode,
        WalletValidationMode::WalletName | WalletValidationMode::Both
    );
    let should_check_address = matches!(
        mode,
        WalletValidationMode::Address | WalletValidationMode::Both
    );

    if should_check_wallet {
        let name = wallet_name.ok_or_else(|| {
            BridgeCliError::Eyre(eyre::eyre!("Wallet name is required for validation"))
        })?;
        if wallet_exists(name)? {
            return Err(BridgeCliError::WalletAlreadyExists(name.to_string()));
        }
    }

    if should_check_address {
        let addr = address.ok_or_else(|| {
            BridgeCliError::Eyre(eyre::eyre!("Address is required for validation"))
        })?;
        let network = network.ok_or_else(|| {
            BridgeCliError::Eyre(eyre::eyre!("Network is required for address validation"))
        })?;
        if address_exists(addr, network)? {
            return Err(BridgeCliError::AddressAlreadyExists(addr.to_string()));
        }
    }

    Ok(())
}

/// Parse and validate an imported wallet file
pub(crate) fn parse_and_validate_imported_wallet(
    file_path: &str,
    wallet_name: &str,
) -> Result<crate::wallet::wallet_storage::GenericWalletData, BridgeCliError> {
    use std::fs;
    use std::path::Path;

    let source_path = Path::new(file_path);

    if !source_path.exists() {
        return Err(BridgeCliError::WalletFileNotFound(file_path.to_string()));
    }

    if !source_path.is_file() {
        return Err(BridgeCliError::PathNotAFile(file_path.to_string()));
    }

    // Read and parse the wallet file
    let wallet_content = fs::read_to_string(source_path)?;
    let wallet_data: crate::wallet::wallet_storage::GenericWalletData =
        serde_json::from_str(&wallet_content).map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!(
                "Failed to parse wallet file '{}': {}",
                source_path.display(),
                e
            ))
        })?;

    // Extract and validate required fields
    let wallet_address = wallet_data.address.clone();
    let network = parse_network(&wallet_data.network)?;

    // Validate that both wallet name and address don't already exist
    validate_wallet_availability(
        Some(wallet_name),
        Some(&wallet_address),
        Some(network),
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
