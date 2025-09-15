#![allow(clippy::result_large_err)]

use crate::errors::BridgeCliError;
use eyre::Context;
use eyre::Result;
use std::path::PathBuf;
use std::str::FromStr;

mod api_utils;
mod backend;
mod bitcoin_merkle;
mod bitcoin_utils;
pub mod cli;
pub mod cli_macros;
pub mod config;
pub mod deposit;
pub mod errors;
mod parameters;
mod script;
mod secure_display;
mod secure_types;
pub mod structs;
pub mod types;
pub mod wallet;
pub mod withdraw;

// Re-export essential public API functions only
pub use bitcoin::address::{NetworkChecked, NetworkUnchecked};

// Wallet operations
pub use wallet::{
    backup_wallet, create_encrypted_wallet, get_mnemonic_from_wallet, get_private_key_from_wallet,
    get_registry_wallet_set, import_wallet_from_file, import_wallet_from_mnemonic,
    import_wallet_from_private_key, print_all_wallets_with_addresses, scan_wallet_files,
};

// Deposit operations
pub use deposit::{
    RecoveryTxParams, create_signed_recovery_tx, get_deposit_params, verify_recovery_tx,
};

// Withdrawal operations
pub use withdraw::{generate_withdrawal_signatures, safe_withdraw, send_safe_withdrawal};

// API utilities
pub use api_utils::broadcast_recovery_tx;

// Constants
pub use bitcoin_utils::SATS_TO_WEI_MULTIPLIER;

pub type BitcoinAddress<V = bitcoin::address::NetworkChecked> = bitcoin::Address<V>;
pub type CitreaAddress = alloy::primitives::Address;

pub fn parse_citrea_address(citrea_address: &str) -> Result<CitreaAddress, BridgeCliError> {
    Ok(CitreaAddress::from_str(citrea_address).wrap_err("Invalid Citrea address format")?)
}

pub(crate) fn get_clementine_home_dir() -> Result<PathBuf, BridgeCliError> {
    let home_dir = dirs::home_dir().ok_or(BridgeCliError::HomeDirectoryNotFound)?;
    Ok(home_dir.join(".clementine"))
}
