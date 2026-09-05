#![allow(clippy::result_large_err)]

use crate::core::errors::BridgeCliError;
use eyre::Result;
use std::path::PathBuf;

mod btc;
mod core;
pub mod deposit;
mod services;
pub mod sqlite_db;
pub mod wallet;
pub mod withdraw;

pub mod config {
    pub use crate::core::config::{
        BitcoinConfig, BridgeCliConfig, ConfigErrors, NetworkConfigs, UNSPENDABLE_XONLY_PUBKEY,
        default_networks, write_config_to,
    };
}

pub mod errors {
    pub use crate::core::errors::BridgeCliError;
}

pub mod secure_types {
    pub use crate::core::secure_types::{SecureKeypair, SecureSecretKey, SecureString};
}

pub mod secure_display {
    pub use crate::core::secure_display::{
        display_mnemonic_securely, display_private_key_securely,
    };
}

pub mod musig2 {
    pub use crate::core::musig2::{aggregate_public_keys, aggregate_public_keys_from_str};
}

pub use crate::btc::utils::parse_transaction_hex;
pub use crate::services::api::{
    MempoolTx, UtxoInfo, broadcast_recovery_tx, get_block_height_for_tx, get_current_block_height,
    get_mempool_txs, get_tx_details, get_utxos,
};
pub use crate::services::backend::{
    backend_deposit_status, backend_withdrawal_status, send_withdrawal_signature_to_operators,
};

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
