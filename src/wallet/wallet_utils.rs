use crate::BitcoinAddress;
use crate::bitcoin_utils::calculate_taproot_address;
use crate::errors::BridgeCliError;
use crate::structs::SecureString;
use crate::wallet::address::parse_address;
use crate::wallet::encryption::aes_decrypt_secure;
use crate::wallet::wallet_storage::get_storage_dir;
use bitcoin::Network;
// use bitcoin::address::NetworkChecked;
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
) -> Result<Keypair, BridgeCliError> {
    if !wallet_exists(wallet_name)? {
        return Err(BridgeCliError::WalletNotFound(wallet_name.to_string()));
    }

    // Load wallet data
    let wallet_data = crate::wallet::wallet_storage::load_wallet_data(wallet_name)?;

    // let wallet_address = load_address(wallet_name, Some(wallet_data.clone()), network)?;

    // Load the encrypted private key
    let encrypted_private_key = wallet_data
        .encrypted_private_key
        .ok_or_else(|| BridgeCliError::NoEncryptedPrivateKeyFound)?;

    let encrypted_data =
        crate::wallet::encryption::encrypted_data_from_hex(&encrypted_private_key)?;
    let decrypted_key = aes_decrypt_secure(&encrypted_data, passphrase)?;

    let secp = Secp256k1::new();
    let mut secret_key = SecretKey::from_str(decrypted_key.expose_secret())?;

    let keypair = Keypair::from_secret_key(&secp, &secret_key);

    secret_key.non_secure_erase();

    Ok(keypair)
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
                    Ok(mut private_key) => {
                        let keypair =
                            Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &private_key);
                        let derived_address = calculate_taproot_address(&keypair, network);

                        // Zeroize the private key after use
                        private_key.non_secure_erase();

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

/// Check if the given address belongs to any wallet in the wallet registry
pub(crate) fn is_wallet_address(address: &str) -> Result<bool, BridgeCliError> {
    let storage_dir = get_storage_dir()?;
    let wallets_file = storage_dir.join("wallets.json");

    if !wallets_file.exists() {
        return Ok(false);
    }

    let wallets_content = std::fs::read_to_string(&wallets_file)?;
    let wallets: HashMap<String, serde_json::Value> = serde_json::from_str(&wallets_content)?;

    for (_wallet_name, wallet_data) in wallets {
        if let Some(wallet_address) = wallet_data.get("address").and_then(|a| a.as_str())
            && wallet_address == address
        {
            return Ok(true);
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
