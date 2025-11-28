#![allow(clippy::result_large_err)]

use crate::errors::BridgeCliError;
use bitcoin::consensus::deserialize;
use eyre::Result;
use std::path::PathBuf;
use std::str::FromStr;

mod api_utils;
mod backend;
mod bitcoin_merkle;
mod bitcoin_utils;
pub mod cli;
pub mod cli_macros;
pub mod cli_network;
pub mod config;
pub mod deposit;
pub mod errors;
mod parameters;
mod script;
mod secure_display;
mod secure_types;
pub mod structs;
pub mod types;
mod utils;
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

pub use cli_macros::handle_err;

pub type BitcoinAddress<V = bitcoin::address::NetworkChecked> = bitcoin::Address<V>;
pub type CitreaAddress = alloy::primitives::Address;

pub fn parse_citrea_address(citrea_address: &str) -> Result<CitreaAddress, BridgeCliError> {
    CitreaAddress::from_str(citrea_address)
        .map_err(|_| BridgeCliError::Eyre(eyre::eyre!("Invalid Citrea address format")))
}

pub(crate) fn get_clementine_home_dir_with_existence_check() -> Result<PathBuf, BridgeCliError> {
    let home_dir = get_clementine_home_dir()?;
    if !home_dir.exists() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Clementine home directory not found at {:?}. Please run 'clementine-cli init' to create one.",
            home_dir
        )));
    }
    Ok(home_dir)
}

pub(crate) fn get_clementine_home_dir() -> Result<PathBuf, BridgeCliError> {
    let home_dir = dirs::home_dir().ok_or(BridgeCliError::HomeDirectoryNotFound)?;
    Ok(home_dir.join(".clementine"))
}

pub(crate) fn get_clementine_config_path_with_existence_check() -> Result<PathBuf, BridgeCliError> {
    let home_dir = get_clementine_home_dir()?;
    let config_path = home_dir.join("bridge_cli_config.toml");
    if !config_path.exists() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Configuration file not found at {:?}. Please run 'clementine-cli init' to create one.",
            config_path
        )));
    }
    Ok(config_path)
}

pub fn parse_transaction_hex(
    tx_hex: &str,
) -> Result<bitcoin::Transaction, BridgeCliError> {
    let tx_bytes = hex::decode(tx_hex).map_err(|e| {
        BridgeCliError::HexDecodeError {
            source: e,
            hex_string: tx_hex.to_string(),
        }
    })?;
    let transaction: bitcoin::Transaction =
        deserialize(&tx_bytes).map_err(|e| BridgeCliError::TransactionDeserializeError {
            source: e,
            tx_hex: tx_hex.to_string(),
        })?;
    Ok(transaction)
}
