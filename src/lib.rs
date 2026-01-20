#![allow(clippy::result_large_err)]

use crate::core::errors::BridgeCliError;
use eyre::Result;
use std::path::PathBuf;
use std::str::FromStr;

pub mod btc;
pub mod cli;
pub mod core;
pub mod deposit;
pub mod services;
pub mod wallet;
pub mod withdraw;
// Re-export essential public API functions only
pub use bitcoin::address::{NetworkChecked, NetworkUnchecked};
pub mod sqlite_db;

pub use btc::utils::SATS_TO_WEI_MULTIPLIER;
pub use btc::utils::parse_transaction_hex;
pub use cli::macros::handle_err;
pub use services::api::broadcast_recovery_tx;

// Wallet operations
pub use wallet::{
    backup_wallet, create_encrypted_wallet, get_mnemonic_from_wallet, get_private_key_from_wallet,
    import_wallet_from_file, import_wallet_from_mnemonic, import_wallet_from_private_key,
    print_all_wallets_with_addresses,
};

// Deposit operations
pub use deposit::{
    RecoveryTxParams, create_signed_recovery_tx, get_deposit_params, verify_recovery_tx,
};

// Withdrawal operations
pub use withdraw::{generate_withdrawal_signatures, safe_withdraw, send_safe_withdrawal};

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
