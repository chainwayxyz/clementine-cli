use bitcoin::Network;
use bitcoin::address::NetworkChecked;
use bitcoin::secp256k1::{Keypair, SecretKey};
use colored::Colorize;
use zeroize::Zeroize;

use crate::BitcoinAddress;
use crate::bitcoin_utils::calculate_taproot_address;
use crate::errors::BridgeCliError;
use crate::mnemonic::get_master_seed_from_mnemonic;
use crate::structs::SecureString;
use std::str::FromStr;

/// Generate a Bitcoin address from a mnemonic phrase
pub(crate) fn generate_address_from_mnemonic_secure(
    secure_mnemonic: &SecureString,
    network: Network,
) -> Result<String, BridgeCliError> {
    let mut master_seed = get_master_seed_from_mnemonic(secure_mnemonic)
        .map_err(|e| BridgeCliError::MnemonicToSeedError(e.to_string()))?;

    let mut master_private_key = SecretKey::from_slice(&master_seed)?;
    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &master_private_key);

    let address = calculate_taproot_address(&keypair, network);

    master_seed.zeroize();
    master_private_key.non_secure_erase();

    Ok(address.to_string())
}

/// Parse a Bitcoin address string into a proper Address object
pub(crate) fn parse_address(
    address: &str,
    network: Network,
) -> Result<BitcoinAddress, BridgeCliError> {
    use crate::{BitcoinAddress, NetworkUnchecked};

    let unchecked_address: BitcoinAddress<NetworkUnchecked> = address
        .parse()
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Failed to parse Bitcoin address: {}", e)))?;

    let address = unchecked_address.require_network(network)?;
    Ok(address)
}

/// Parse a taproot address specifically and validate it's the correct type
pub(crate) fn parse_taproot_address(
    address: &str,
    network: Network,
) -> Result<BitcoinAddress, BridgeCliError> {
    use bitcoin::AddressType;

    let address = parse_address(address, network)?;

    // Verify it's a taproot (P2TR) address
    if address.address_type() != Some(AddressType::P2tr) {
        return Err(BridgeCliError::NotTaprootAddress);
    }

    Ok(address)
}

/// Get all wallets with their names and addresses from storage and print them
pub fn get_all_wallets_with_addresses() -> Result<(), BridgeCliError> {
    use std::fs;

    let storage_dir = crate::wallet_storage::get_storage_dir()
        .map_err(|e| BridgeCliError::StorageDirectoryError(e.to_string()))?;

    if !storage_dir.exists() {
        println!(
            "Storage directory does not exist: {}",
            storage_dir.display()
        );
        return Ok(());
    }

    let wallets_file = storage_dir.join("wallets.json");
    if wallets_file.exists() {
        let wallets_content = fs::read_to_string(&wallets_file)
            .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Failed to read wallets file: {}", e)))?;

        let wallets: serde_json::Value = serde_json::from_str(&wallets_content).map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!("Failed to parse wallets JSON: {}", e))
        })?;

        for (name, wallet) in wallets.as_object().unwrap_or(&serde_json::Map::new()) {
            if let Some(address) = wallet.get("address").and_then(|a| a.as_str()) {
                println!("Wallet: {} -> Address: {}", name.blue(), address.green());
            } else {
                eprintln!("Warning: Wallet '{}' does not have an address field", name);
            }
        }
        return Ok(());
    }

    Ok(())
}

pub(crate) fn str_to_address(
    address: &str,
    network: Network,
) -> Result<BitcoinAddress<NetworkChecked>, BridgeCliError> {
    let wallet_address = BitcoinAddress::from_str(address)
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Invalid wallet address: {}", e)))?;

    let wallet_address = wallet_address
        .require_network(network)
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Address does not match network: {}", e)))?;

    Ok(wallet_address)
}
