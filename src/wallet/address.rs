//! Bitcoin address utilities and wallet management for Clementine CLI.
//!
//! This module provides functionality for:
//! - Generating Bitcoin addresses from mnemonic phrases using Taproot (P2TR)
//! - Computing Taproot addresses directly from keypairs
//! - Parsing and validating Bitcoin addresses with network verification
//! - Managing wallet purposes (deposit vs withdrawal) with address prefixes
//! - Listing and displaying stored wallets with their associated addresses
//!
//! ## Address Types
//!
//! The module focuses on Taproot addresses (P2TR) which are generated from:
//! - BIP39 mnemonic phrases converted to master seeds
//! - Secp256k1 keypairs derived from the master private key
//! - Network-specific address generation (mainnet, testnet, etc.)
//!
//! ## Purpose-based Prefixes
//!
//! Addresses are categorized by purpose with specific prefixes:
//! - **Deposit addresses**: Prefixed with "dep"
//! - **Withdrawal addresses**: Prefixed with "wit"
//!

use bip39::Mnemonic;
use bitcoin::address::NetworkChecked;
use bitcoin::secp256k1::{Keypair, SecretKey};
use bitcoin::{AddressType, Network};
use clap::ValueEnum;
use colored::Colorize;
use secrecy::ExposeSecret;

use crate::bitcoin_utils::SECP;
use crate::errors::BridgeCliError;
use crate::structs::{SecureKeypair, SecureSecretKey, TaprootAddressWithPrefix};
use crate::wallet::mnemonic::get_master_seed_from_mnemonic;
use crate::wallet::wallet_storage::{get_storage_dir, get_wallets_from_registry};
use crate::wallet::wallet_utils::parse_network;
use crate::{BitcoinAddress, NetworkUnchecked};
use chrono::{DateTime, TimeZone, Utc};

const DEPOSIT_PREFIX: &str = "dep";
const WITHDRAWAL_PREFIX: &str = "wit";

/// Purpose for the wallet. Can be for either `deposit` or `withdrawal`.
/// This affects the prefix of the generated address.
/// If `withdrawal`, the address will be prefixed with "wit".
/// If `deposit`, it will be prefixed with "dep".
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Hash)]
pub enum Purpose {
    Deposit,
    Withdrawal,
}

impl Purpose {
    pub fn to_prefix(&self) -> &str {
        match self {
            Purpose::Deposit => DEPOSIT_PREFIX,
            Purpose::Withdrawal => WITHDRAWAL_PREFIX,
        }
    }

    pub fn purpose_from_str(s: &str) -> Result<Self, BridgeCliError> {
        match s.to_lowercase().as_str() {
            DEPOSIT_PREFIX => Ok(Purpose::Deposit),
            WITHDRAWAL_PREFIX => Ok(Purpose::Withdrawal),
            _ => Err(BridgeCliError::InvalidPrefix(s.to_string())),
        }
    }
}

/// Generate a Bitcoin address from a mnemonic phrase
pub(crate) fn generate_address_from_mnemonic(
    mnemonic: &Mnemonic,
    network: Network,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    let master_seed = get_master_seed_from_mnemonic(mnemonic)
        .map_err(|e| BridgeCliError::MnemonicToSeedError(e.to_string()))?;

    let master_private_key =
        SecureSecretKey::new(SecretKey::from_slice(master_seed.expose_secret())?);
    let keypair = SecureKeypair::new(Keypair::from_secret_key(
        &SECP,
        master_private_key.as_ref_inner(),
    ));

    let address = calculate_taproot_address(&keypair, network);

    let address = TaprootAddressWithPrefix::new(address, purpose)?;

    Ok(address)
}

/// Calculate taproot address from a keypair
pub(crate) fn calculate_taproot_address(
    keypair: &SecureKeypair,
    network: Network,
) -> BitcoinAddress {
    let (xonly_public_key, _parity) = keypair.as_ref().public_key().x_only_public_key();
    BitcoinAddress::p2tr(&SECP, xonly_public_key, None, network)
}

/// Parse a Bitcoin address string into a proper Address object
pub fn parse_address(address: &str, network: Network) -> Result<BitcoinAddress, BridgeCliError> {
    let unchecked_address: BitcoinAddress<NetworkUnchecked> = address
        .parse()
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Failed to parse Bitcoin address: {}", e)))?;

    let address = unchecked_address.require_network(network)?;
    Ok(address)
}

/// Parse a taproot address specifically and validate it's the correct type
pub fn parse_taproot_address(
    address: &str,
    network: Network,
) -> Result<BitcoinAddress, BridgeCliError> {
    let address = parse_address(address, network)?;

    // Verify it's a taproot (P2TR) address
    if address.address_type() != Some(AddressType::P2tr) {
        return Err(BridgeCliError::NotTaprootAddress(address.to_string()));
    }

    Ok(address)
}

/// Get all wallets with their names and addresses from storage and print them
pub fn print_all_wallets_with_addresses() -> Result<(), BridgeCliError> {
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

    let mut wallets: Vec<_> = wallets.into_values().collect();
    wallets.sort_by_key(|w| {
        DateTime::parse_from_rfc3339(&w.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc.timestamp_opt(0, 0).single().unwrap())
    });

    let (mainnet, others): (Vec<_>, Vec<_>) =
        wallets.into_iter().partition(|w| w.network == "bitcoin");

    fn print_wallet_section(
        section_title: &str,
        wallets: &[crate::wallet::wallet_storage::WalletRegistryEntry],
    ) -> Result<(), BridgeCliError> {
        if wallets.is_empty() {
            return Ok(());
        }
        println!("{}", section_title.bold().underline());
        for wallet_entry in wallets {
            let network = parse_network(&wallet_entry.network)?;
            let address = TaprootAddressWithPrefix::from_string_with_prefix(
                &wallet_entry.addres_with_prefix,
                network,
            )?;
            let import_info = if let Some(true) = wallet_entry.imported {
                if let Some(method) = &wallet_entry.import_method {
                    format!(", (Imported via {})", method)
                } else {
                    ", (Imported)".to_string()
                }
            } else {
                "".to_string()
            };
            let network = format!("Network: {}", wallet_entry.network);
            println!(
                "Label: {} -> Address: {}, {}{}",
                &wallet_entry.label,
                &address.address_with_prefix(),
                network,
                import_info,
            );
        }
        Ok(())
    }

    print_wallet_section("Wallets on networks other than Bitcoin mainnet:", &others)?;

    println!();

    print_wallet_section("Wallets on Bitcoin mainnet:", &mainnet)?;

    Ok(())
}

pub fn should_not_have_purpose(address: &str) -> Result<(), BridgeCliError> {
    address.get(0..3).map_or(Ok(()), |prefix| match prefix {
        DEPOSIT_PREFIX | WITHDRAWAL_PREFIX => Err(BridgeCliError::AddressShouldNotHavePrefix(
            address.to_string(),
        )),
        _ => Ok(()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::{AddressType, Network};

    #[test]
    fn test_parse_taproot_address_valid() {
        let addr_str = "bc1pdqrcrxa8vx6gy75mfdfj84puhxffh4fq46h3gkp6jxdd0vjcsdyspfxcv6";
        let addr = parse_taproot_address(addr_str, Network::Bitcoin).unwrap();
        assert_eq!(addr.address_type(), Some(AddressType::P2tr));
    }

    #[test]
    fn test_parse_taproot_address_invalid_type() {
        let non_taproot = "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx"; // P2WPKH
        assert!(parse_taproot_address(non_taproot, Network::Testnet4).is_err());
    }

    #[test]
    fn test_parse_taproot_address_wrong_network() {
        let mainnet_addr = "bc1pqqqqp399et2xygdj5xreqhjjvcmzhxw4aywxecjdzew6hylgvsesf3hn0c";
        assert!(parse_taproot_address(mainnet_addr, Network::Testnet4).is_err());
    }

    #[test]
    fn test_parse_taproot_address_invalid_format() {
        let invalid = "invalid_address";
        assert!(parse_taproot_address(invalid, Network::Testnet4).is_err());
    }
}
