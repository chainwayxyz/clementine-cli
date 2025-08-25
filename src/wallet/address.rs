use bitcoin::address::NetworkChecked;
use bitcoin::secp256k1::{Keypair, SecretKey};
use bitcoin::{AddressType, Network};
use colored::Colorize;
use secrecy::ExposeSecret;

use crate::bitcoin_utils::{SECP, calculate_taproot_address};
use crate::errors::BridgeCliError;
use crate::structs::{SecureKeypair, SecureSecretKey, SecureString};
use crate::wallet::mnemonic::get_master_seed_from_mnemonic;
use crate::wallet::wallet_storage::{get_storage_dir, get_wallets_from_registry};
use crate::{BitcoinAddress, NetworkUnchecked};
use std::str::FromStr;

/// Generate a Bitcoin address from a mnemonic phrase
pub(crate) fn generate_address_from_mnemonic_secure(
    secure_mnemonic: &SecureString,
    network: Network,
) -> Result<String, BridgeCliError> {
    let master_seed = get_master_seed_from_mnemonic(secure_mnemonic)
        .map_err(|e| BridgeCliError::MnemonicToSeedError(e.to_string()))?;

    let master_private_key =
        SecureSecretKey::new(SecretKey::from_slice(master_seed.expose_secret())?);
    let keypair = SecureKeypair::new(Keypair::from_secret_key(&SECP, master_private_key.as_ref()));

    let address = calculate_taproot_address(&keypair, network);

    Ok(address.to_string())
}

/// Parse a Bitcoin address string into a proper Address object
pub(crate) fn parse_address(
    address: &str,
    network: Network,
) -> Result<BitcoinAddress, BridgeCliError> {
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
    let address = parse_address(address, network)?;

    // Verify it's a taproot (P2TR) address
    if address.address_type() != Some(AddressType::P2tr) {
        return Err(BridgeCliError::NotTaprootAddress);
    }

    Ok(address)
}

/// Get all wallets with their names and addresses from storage and print them
pub fn get_all_wallets_with_addresses() -> Result<(), BridgeCliError> {
    let storage_dir = get_storage_dir()?;

    if !storage_dir.exists() {
        println!(
            "Storage directory does not exist: {}",
            storage_dir.display()
        );
        return Ok(());
    }

    let wallets = get_wallets_from_registry()?;

    if wallets.is_empty() {
        println!("No wallets found.");
        return Ok(());
    }

    println!("Found {} wallet(s):", wallets.len());
    for (name, wallet_entry) in &wallets {
        let import_info = if let Some(true) = wallet_entry.imported {
            if let Some(method) = &wallet_entry.import_method {
                format!(" (Imported via {})", method)
            } else {
                " (Imported)".to_string()
            }
        } else {
            "".to_string()
        };

        println!(
            "Wallet: {} -> Address: {}{}",
            name.blue(),
            wallet_entry.address.green(),
            import_info.cyan()
        );
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
