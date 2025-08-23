use crate::BitcoinAddress;
use crate::bitcoin_utils::calculate_taproot_address;
use crate::errors::BridgeCliError;
use crate::structs::{SecureKeypair, SecureSecretKey, SecureString};
use crate::wallet::address::str_to_address;
use crate::wallet::encryption::aes_decrypt_secure;
use crate::wallet::wallet_storage::{GenericWalletData, get_storage_dir, load_wallet_data};
use bitcoin::Network;
use bitcoin::address::NetworkChecked;
use bitcoin::key::Keypair;
use bitcoin::secp256k1::Secp256k1;
use bitcoin::secp256k1::SecretKey;
use eyre::eyre;
use secrecy::ExposeSecret;
use std::str::FromStr;

/// Securely load a key from wallet storage - always requires a passphrase
pub(crate) fn load_key_and_address(
    wallet_name: &str,
    network: Network,
    passphrase: &SecureString,
) -> Result<(SecureKeypair, BitcoinAddress<NetworkChecked>), BridgeCliError> {
    if !wallet_exists(wallet_name)? {
        return Err(BridgeCliError::WalletNotFound(wallet_name.to_string()));
    }

    // Load wallet data
    let wallet_data = load_wallet_data(wallet_name)?;

    let wallet_address = load_address(wallet_name, Some(wallet_data.clone()), network)?;

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

    Ok((secure_keypair, wallet_address))
}

pub(crate) fn load_address(
    wallet_name: &str,
    generic_wallet_data: Option<GenericWalletData>,
    network: Network,
) -> Result<BitcoinAddress<NetworkChecked>, BridgeCliError> {
    let wallet_data = if let Some(data) = generic_wallet_data {
        data
    } else {
        crate::wallet::wallet_storage::load_wallet_data(wallet_name)?
    };

    let wallet_network = parse_network(wallet_data.network.as_str())?;
    check_network_compatibility(wallet_network, network)?;

    let address_str = wallet_data.address.as_str();
    let address = str_to_address(address_str, network)
        .map_err(|e| BridgeCliError::Eyre(eyre!("Invalid wallet address: {}", e)))?;

    Ok(address)
}

/// Helper function to validate mnemonic imports during wallet import
pub(crate) fn validate_mnemonic_import(
    decrypted_mnemonic: &SecureString,
    wallet_data: &serde_json::Value,
    wallet_address: &str,
) -> Result<(), BridgeCliError> {
    let network_str = wallet_data["network"].as_str().unwrap_or("mainnet");
    let network = parse_network(network_str)?;

    // Generate address from mnemonic to verify it matches
    match crate::wallet::address::generate_address_from_mnemonic_secure(decrypted_mnemonic, network)
    {
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
    wallet_data: &serde_json::Value,
    passphrase: &SecureString,
    wallet_address: &str,
) -> Result<(), BridgeCliError> {
    if wallet_data["encrypted_private_key"].as_object().is_some() {
        let encrypted_private_key_hex: crate::wallet::encryption::EncryptedDataHex =
            serde_json::from_value(wallet_data["encrypted_private_key"].clone()).map_err(|e| {
                BridgeCliError::Eyre(eyre!(
                    "Failed to parse encrypted private key structure: {}",
                    e
                ))
            })?;

        let encrypted_private_data =
            crate::wallet::encryption::encrypted_data_from_hex(&encrypted_private_key_hex)
                .map_err(|e| {
                    BridgeCliError::Eyre(eyre!("Failed to parse encrypted private key: {}", e))
                })?;

        // Decrypt and validate the private key
        match aes_decrypt_secure(&encrypted_private_data, passphrase) {
            Ok(decrypted_private_key) => {
                let network_str = wallet_data["network"].as_str().unwrap_or("mainnet");
                let network = parse_network(network_str)?;

                // Validate the private key format and derive address to verify
                match SecretKey::from_str(decrypted_private_key.expose_secret()) {
                    Ok(private_key) => {
                        let keypair = SecureKeypair::new(Keypair::from_secret_key(
                            &crate::bitcoin_utils::SECP,
                            &private_key,
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
    let wallets = crate::wallet::wallet_storage::get_wallets_from_registry()?;

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
