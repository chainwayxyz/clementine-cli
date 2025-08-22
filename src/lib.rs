use eyre::Result;
use std::env;
use std::str::FromStr;

pub type BitcoinAddress<V = bitcoin::address::NetworkChecked> = bitcoin::Address<V>;
pub use bitcoin::address::{NetworkChecked, NetworkUnchecked};
use eyre::Context;

use crate::errors::BridgeCliError;

pub type CitreaAddress = alloy::primitives::Address;

/// Check if debug mode is enabled via CLEMENTINE_DEBUG environment variable
pub fn is_debug_enabled() -> bool {
    env::var("CLEMENTINE_DEBUG").is_ok()
}

/// Debug macro that only prints when CLEMENTINE_DEBUG is set
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        {
            use colored::*;
            if $crate::is_debug_enabled() {
                println!("{} {}", "DEBUG".magenta().bold(), format!($($arg)*));
            }
        }
    };
}

/// Debug macro for colored output that only prints when CLEMENTINE_DEBUG is set
#[macro_export]
macro_rules! debug_colored {
    ($color:expr, $($arg:tt)*) => {
        {
            use colored::*;
            if $crate::is_debug_enabled() {
                println!("{} {}", "DEBUG".magenta().bold(), format!($($arg)*).color($color));
            }
        }
    };
}
mod backend;
mod bitcoin_merkle;
mod bitcoin_utils;
pub mod config;
pub mod deposit;
pub mod errors;
mod musig2;
mod parameters;
mod script;
mod secure_display;
mod structs;
pub mod types;
pub mod wallet;
pub mod withdrawal;

// Re-export commonly used address functions for public API
pub use wallet::get_all_wallets_with_addresses;
pub use wallet::show_mnemonic_secure;

pub fn parse_citrea_address(citrea_address: &str) -> Result<CitreaAddress, BridgeCliError> {
    Ok(CitreaAddress::from_str(citrea_address).wrap_err("Invalid Citrea address format")?)
}
