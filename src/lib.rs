use crate::errors::BridgeCliError;
use eyre::Context;
use eyre::Result;
use std::str::FromStr;

mod backend;
mod bitcoin_merkle;
mod bitcoin_utils;
pub mod config;
pub mod deposit;
pub mod errors;
mod parameters;
mod script;
mod secure_display;
mod structs;
pub mod types;
pub mod wallet;
pub mod withdrawal;

// Re-export commonly used address functions for public API
pub use bitcoin::address::{NetworkChecked, NetworkUnchecked};
pub use wallet::get_all_wallets_with_addresses;
pub use wallet::show_mnemonic_secure;

pub type BitcoinAddress<V = bitcoin::address::NetworkChecked> = bitcoin::Address<V>;
pub type CitreaAddress = alloy::primitives::Address;

pub fn parse_citrea_address(citrea_address: &str) -> Result<CitreaAddress, BridgeCliError> {
    Ok(CitreaAddress::from_str(citrea_address).wrap_err("Invalid Citrea address format")?)
}
