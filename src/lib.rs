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

// Re-export essential public API functions only
pub use bitcoin::address::{NetworkChecked, NetworkUnchecked};

// Wallet operations
pub use wallet::{
    backup_wallet, create_encrypted_wallet_with_address, delete_wallet,
    get_all_wallets_with_addresses, import_wallet_from_file, import_wallet_from_mnemonic,
    import_wallet_from_private_key, show_mnemonic, show_private_key, verify_wallet_integrity,
};

// Deposit operations
pub use deposit::{get_deposit_address, get_deposit_params, sign_recovery_tx, verify_recovery_tx};

// Withdrawal operations
pub use withdrawal::{generate_withdrawal_signature, safe_withdraw, send_safe_withdrawal};

pub type BitcoinAddress<V = bitcoin::address::NetworkChecked> = bitcoin::Address<V>;
pub type CitreaAddress = alloy::primitives::Address;

pub fn parse_citrea_address(citrea_address: &str) -> Result<CitreaAddress, BridgeCliError> {
    Ok(CitreaAddress::from_str(citrea_address).wrap_err("Invalid Citrea address format")?)
}
