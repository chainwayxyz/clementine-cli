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

use std::str::FromStr;

use crate::btc::utils::SECP;
use crate::core::errors::BridgeCliError;
use crate::core::secure_types::{SecureKeypair, SecureSecretKey};
use crate::sqlite_db::sqlite_client::SqliteDb;
use crate::sqlite_db::wallet_db::{MinimalWalletData, WalletTable};
use crate::wallet::BitcoinAddress;
use crate::wallet::mnemonic::get_master_seed_from_mnemonic;
use bip39::Mnemonic;
use bitcoin::address::{NetworkChecked, NetworkUnchecked, NetworkValidation};
use bitcoin::secp256k1::{Keypair, SecretKey};
use bitcoin::{Address, AddressType, Network};
use clap::ValueEnum;
use colored::Colorize;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};

const DEPOSIT_PREFIX: &str = "dep";
const WITHDRAWAL_PREFIX: &str = "wit";

/// Purpose for the wallet. Can be for either `deposit` or `withdrawal`.
/// This affects the prefix of the generated address.
/// If `withdrawal`, the address will be prefixed with "wit".
/// If `deposit`, it will be prefixed with "dep".
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaprootAddressWithPrefix<T: NetworkValidation> {
    pub address: Address<T>,
    pub purpose: Purpose,
}

impl TaprootAddressWithPrefix<NetworkChecked> {
    pub fn new(address: Address<NetworkChecked>, purpose: Purpose) -> Result<Self, BridgeCliError> {
        let address_type = if let Some(t) = address.address_type() {
            t
        } else {
            return Err(BridgeCliError::InvalidAddressFormat(address.to_string()));
        };

        if address_type != bitcoin::AddressType::P2tr {
            return Err(BridgeCliError::InvalidAddressFormat(address.to_string()));
        }

        Ok(Self { address, purpose })
    }

    pub fn from_string_with_prefix(
        address: &str,
        network: bitcoin::Network,
    ) -> Result<Self, BridgeCliError> {
        if address.len() < 3 {
            return Err(BridgeCliError::InvalidAddressFormat(address.to_string()));
        }

        let purpose = Purpose::purpose_from_str(&address[0..3])?;

        let addr_str = &address[3..];

        let bitcoin_address = parse_taproot_address(addr_str, network)?;

        let taproot_address_with_prefix = Self::new(bitcoin_address, purpose)?;

        Ok(taproot_address_with_prefix)
    }

    pub fn from_string_without_prefix(
        address: &str,
        purpose: Purpose,
        network: bitcoin::Network,
    ) -> Result<Self, BridgeCliError> {
        let bitcoin_address = parse_taproot_address(address, network)?;
        let taproot_address_with_prefix = Self::new(bitcoin_address, purpose)?;
        Ok(taproot_address_with_prefix)
    }
}

impl TaprootAddressWithPrefix<NetworkUnchecked> {
    pub fn from_string_with_prefix_unchecked(address: &str) -> Result<Self, BridgeCliError> {
        if address.len() < 4 {
            return Err(BridgeCliError::InvalidAddressFormat(address.to_string()));
        }

        let purpose = Purpose::purpose_from_str(&address[0..3])?;

        let addr_str = &address[3..];

        let unchecked_address: BitcoinAddress<NetworkUnchecked> =
            addr_str.parse().map_err(|e| {
                BridgeCliError::Eyre(eyre::eyre!("Failed to parse Bitcoin address: {}", e))
            })?;

        let taproot_address_with_prefix = Self {
            address: unchecked_address,
            purpose,
        };

        Ok(taproot_address_with_prefix)
    }

    pub fn assume_checked(&self) -> TaprootAddressWithPrefix<NetworkChecked> {
        TaprootAddressWithPrefix {
            address: self.address.clone().assume_checked(),
            purpose: self.purpose,
        }
    }
}

impl From<&TaprootAddressWithPrefix<NetworkChecked>>
    for TaprootAddressWithPrefix<NetworkUnchecked>
{
    fn from(val: &TaprootAddressWithPrefix<NetworkChecked>) -> Self {
        TaprootAddressWithPrefix {
            address: Address::from_str(&val.address.to_string())
                .expect("Cannot fail since address is valid"),
            purpose: val.purpose,
        }
    }
}

pub trait AddrDisplay {
    fn as_display_str(&self) -> String;
}

impl AddrDisplay for Address<NetworkChecked> {
    fn as_display_str(&self) -> String {
        self.to_string()
    }
}

impl AddrDisplay for Address<NetworkUnchecked> {
    fn as_display_str(&self) -> String {
        self.clone().assume_checked().to_string()
    }
}

impl<T> TaprootAddressWithPrefix<T>
where
    T: NetworkValidation,
    Address<T>: AddrDisplay,
{
    pub fn address_without_prefix(&self) -> String {
        self.address.as_display_str()
    }

    pub fn address_with_prefix(&self) -> String {
        format!(
            "{}{}",
            self.purpose.to_prefix(),
            self.address_without_prefix()
        )
    }
}

/// Generate a Bitcoin address from a mnemonic phrase
pub(crate) fn generate_address_from_mnemonic(
    mnemonic: &Mnemonic,
    network: Network,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    let master_seed = get_master_seed_from_mnemonic(mnemonic);

    let master_private_key = SecureSecretKey::new(
        SecretKey::from_slice(master_seed.expose_secret()).map_err(|e| {
            tracing::error!("Error creating master private key from seed: {}", e);
            BridgeCliError::Eyre(eyre::eyre!("Failed to create master private key from seed"))
        })?,
    );
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

    let address = unchecked_address.require_network(network).map_err(|_| {
        BridgeCliError::Eyre(eyre::eyre!(
            "Address network mismatch: {}, address: {}",
            network,
            address
        ))
    })?;
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
pub async fn print_all_wallets_with_addresses() -> Result<(), BridgeCliError> {
    let db = SqliteDb::open_with_schema().await?;
    let mut wallets: Vec<MinimalWalletData> = WalletTable::get_all_wallets(db.pool()).await?;

    if wallets.is_empty() {
        println!("No wallets found.");
        return Ok(());
    }

    wallets.sort_by_key(|w| w.created_at);

    let (mainnet, others): (Vec<_>, Vec<_>) =
        wallets.into_iter().partition(|w| w.network == "bitcoin");

    fn print_wallet_section(
        section_title: &str,
        wallets: &[MinimalWalletData],
    ) -> Result<(), BridgeCliError> {
        if wallets.is_empty() {
            return Ok(());
        }
        println!("{}", section_title.bold().underline());
        for wallet in wallets {
            let import_info = if wallet.imported {
                if let Some(method) = &wallet.import_method {
                    format!(", (Imported via {})", method)
                } else {
                    ", (Imported)".to_string()
                }
            } else {
                "".to_string()
            };
            let network = format!("Network: {}", wallet.network);
            println!(
                "Label: {} -> Address: {}, {}{}",
                &wallet.label, &wallet.address, network, import_info,
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
