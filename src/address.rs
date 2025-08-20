use anyhow::anyhow;
use bitcoin::Network;
use bitcoin::secp256k1::{Keypair, SecretKey};
use zeroize::Zeroize;

use crate::BitcoinAddress;
use crate::bitcoin_utils::calculate_taproot_address;
use crate::mnemonic::get_master_seed_from_mnemonic;
use crate::secure_structs::SecureString;

/// Generate a Bitcoin address from a mnemonic phrase
pub fn generate_address_from_mnemonic_secure(
    secure_mnemonic: &SecureString,
    network: Network,
) -> Result<String, anyhow::Error> {
    let mut master_seed = get_master_seed_from_mnemonic(secure_mnemonic)
        .map_err(|e| anyhow!("Failed to generate master seed from mnemonic: {}", e))?;

    let mut master_private_key = SecretKey::from_slice(&master_seed)?;
    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &master_private_key);

    let address = calculate_taproot_address(&keypair, network);

    master_seed.zeroize();
    master_private_key.non_secure_erase();

    Ok(address.to_string())
}

/// Parse a Bitcoin address string into a proper Address object
pub fn parse_address(address: &str, network: Network) -> Result<BitcoinAddress, anyhow::Error> {
    use crate::{BitcoinAddress, NetworkUnchecked};

    let unchecked_address: BitcoinAddress<NetworkUnchecked> = address
        .parse()
        .map_err(|_| anyhow!("Invalid Bitcoin address format"))?;
    let address = unchecked_address.require_network(network)?;
    Ok(address)
}

/// Parse a taproot address specifically and validate it's the correct type
pub fn parse_taproot_address(
    address: &str,
    network: Network,
) -> Result<BitcoinAddress, anyhow::Error> {
    use bitcoin::AddressType;

    let address = parse_address(address, network)?;

    // Verify it's a taproot (P2TR) address
    if address.address_type() != Some(AddressType::P2tr) {
        return Err(anyhow!("Address is not a taproot (P2TR) address"));
    }

    Ok(address)
}

/// Extract address from wallet JSON data
pub fn extract_address_from_wallet(
    wallet_data: &serde_json::Value,
) -> Result<String, anyhow::Error> {
    // Try to get the address field from the wallet data
    if let Some(address) = wallet_data.get("address") {
        if let Some(address_str) = address.as_str() {
            return Ok(address_str.to_string());
        }
    }

    Err(anyhow!("Address field not found or invalid in wallet data"))
}

/// Helper function to process a wallet file and extract its address
fn process_wallet_file(file_path: &std::path::Path, file_name: &str) -> Option<String> {
    use std::fs;
    
    let wallet_content = fs::read_to_string(file_path).ok()?;
    let wallet_data: serde_json::Value = serde_json::from_str(&wallet_content).ok()?;
    
    match extract_address_from_wallet(&wallet_data) {
        Ok(address) => Some(address),
        Err(e) => {
            eprintln!("Warning: Failed to extract address from {}: {}", file_name, e);
            None
        }
    }
}

/// Get all wallet addresses from storage and print them
pub fn get_all_wallet_addresses() -> Result<(), anyhow::Error> {
    use std::fs;

    let storage_dir = crate::wallet_storage::get_storage_dir()
        .map_err(|e| anyhow!("Failed to get storage directory: {}", e))?;

    if !storage_dir.exists() {
        println!("No wallets found in storage.");
        return Ok(());
    }

    let addresses: Vec<String> = fs::read_dir(&storage_dir)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();
            
            // Check if it's a wallet file (wallet_ADDRESS.json)
            if file_name_str.starts_with("wallet_") 
                && file_name_str.ends_with(".json") 
                && file_name_str != "wallets.json" {
                process_wallet_file(&entry.path(), &file_name_str)
            } else {
                None
            }
        })
        .collect();

    // Print the addresses
    if addresses.is_empty() {
        println!("No wallets found in storage.");
    } else {
        println!("📂 Found {} wallet(s):", addresses.len());
        println!();
        for (index, address) in addresses.iter().enumerate() {
            println!("{}. {}", index + 1, address);
        }
    }

    Ok(())
}
