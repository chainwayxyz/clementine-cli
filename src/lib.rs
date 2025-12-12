#![allow(clippy::result_large_err)]

use crate::errors::BridgeCliError;
use eyre::Result;
use std::path::PathBuf;
use std::str::FromStr;

pub(crate) mod api_utils;
pub(crate) mod backend;
mod bitcoin_merkle;
pub(crate) mod bitcoin_utils;
pub mod cli;
pub mod cli_macros;
pub mod cli_network;
pub mod config;
pub mod deposit;
pub mod errors;
mod parameters;
mod script;
mod secure_display;
pub(crate) mod secure_types;
pub mod structs;
pub mod types;
mod utils;
pub mod wallet;
pub mod withdraw;
// Re-export essential public API functions only
pub use bitcoin::address::{NetworkChecked, NetworkUnchecked};

// Wallet operations
pub use wallet::{
    backup_wallet, create_encrypted_wallet, get_mnemonic_from_wallet, get_registry_wallet_set,
    import_wallet_from_file, import_wallet_from_mnemonic, import_wallet_from_private_key,
    print_all_wallets_with_addresses, scan_wallet_files,
};
// Note: get_private_key_from_wallet uses SecureSecretKey (pub(crate)) so it's not re-exported

// Deposit operations
pub use deposit::{
    RecoveryTxParams, VerifyRecoveryTxParams, get_deposit_address, get_deposit_params,
    verify_recovery_tx,
};
// Note: create_signed_recovery_tx uses SecureKeypair (pub(crate)) so it's not re-exported

// Withdrawal operations
pub use withdraw::{safe_withdraw, send_safe_withdrawal};
// Note: generate_withdrawal_signatures uses SecureKeypair (pub(crate)) so it's not re-exported

// API utilities
pub use api_utils::broadcast_recovery_tx;

// Config
pub use config::BridgeCliConfig;

// Bitcoin utilities
pub use bitcoin_utils::{SATS_TO_WEI_MULTIPLIER, SECP};

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

#[cfg(any(test, feature = "test-helpers"))]
thread_local! {
    static TEST_TEMP_DIR: std::cell::RefCell<Option<tempfile::TempDir>> = std::cell::RefCell::new(None);
}

/// Clear the test temp directory (test utility function)
///
/// This function is used by tests to reset the temp directory between test runs.
/// This is a no-op in non-test builds where TEST_TEMP_DIR doesn't exist.
#[doc(hidden)]
pub fn clear_test_temp_dir() {
    #[cfg(any(test, feature = "test-helpers"))]
    {
        TEST_TEMP_DIR.with(|temp_dir| {
            *temp_dir.borrow_mut() = None;
        });
    }
}

pub(crate) fn get_clementine_home_dir() -> Result<PathBuf, BridgeCliError> {
    #[cfg(any(test, feature = "test-helpers"))]
    {
        return TEST_TEMP_DIR.with(|temp_dir| {
            let mut temp_dir_ref = temp_dir.borrow_mut();
            if temp_dir_ref.is_none() {
                let temp = tempfile::TempDir::new().expect("Failed to create temp dir");
                let clementine_dir = temp.path().join(".clementine");
                let keys_dir = clementine_dir.join("keys");
                std::fs::create_dir_all(&keys_dir).expect("Failed to create .clementine/keys dir");
                *temp_dir_ref = Some(temp);
            }

            let temp = temp_dir_ref.as_ref().unwrap();
            Ok(temp.path().join(".clementine"))
        });
    }

    #[cfg(not(any(test, feature = "test-helpers")))]
    {
        let home_dir = dirs::home_dir().ok_or(BridgeCliError::HomeDirectoryNotFound)?;
        Ok(home_dir.join(".clementine"))
    }
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

// ============================================================================
// Test-only exports - DO NOT USE IN PRODUCTION CODE
// ============================================================================
// These functions are exposed ONLY for integration testing purposes.
// They are hidden from documentation and should never be used by external code.
// Using #[doc(hidden)] ensures they don't appear in generated docs.
// ============================================================================

#[doc(hidden)]
pub mod __test_helpers {
    use super::*;
    use crate::secure_types::{SecureKeypair, SecureSecretKey, SecureString};
    use crate::structs::{AddrDisplay, DepositStatus, TaprootAddressWithPrefix, WithdrawStatus};
    use crate::wallet::wallet_utils::load_key_with_purpose_check;
    use crate::wallet::{Purpose, get_private_key_from_wallet};
    use bitcoin::{Address, OutPoint};

    // Backend test helpers
    #[doc(hidden)]
    pub async fn test_create_deposit_account(
        citrea_address: &CitreaAddress,
        recovery_taproot_address: &BitcoinAddress,
        config: &BridgeCliConfig,
    ) -> Result<BitcoinAddress, BridgeCliError> {
        crate::backend::create_deposit_account(citrea_address, recovery_taproot_address, config)
            .await
    }

    #[doc(hidden)]
    pub async fn test_backend_deposit_status(
        taproot_address: &Address,
        config: &BridgeCliConfig,
    ) -> Result<Vec<DepositStatus>, BridgeCliError> {
        crate::backend::backend_deposit_status(taproot_address, config).await
    }

    #[doc(hidden)]
    pub async fn test_backend_withdrawal_status(
        withdrawal_outpoint: OutPoint,
        config: &BridgeCliConfig,
    ) -> Result<Vec<WithdrawStatus>, BridgeCliError> {
        crate::backend::backend_withdrawal_status(withdrawal_outpoint, config).await
    }

    #[doc(hidden)]
    pub async fn test_send_withdrawal_signature_to_operators(
        signer_address: &str,
        destination_address: &str,
        withdrawal_outpoint: OutPoint,
        signature: &str,
        config: &BridgeCliConfig,
        amount: u64,
    ) -> Result<(), BridgeCliError> {
        crate::backend::send_withdrawal_signature_to_operators(
            signer_address,
            destination_address,
            withdrawal_outpoint,
            signature,
            config,
            amount,
        )
        .await
    }

    // Bitcoin utility test helpers
    #[doc(hidden)]
    pub fn test_calculate_deposit_address(
        citrea_address: &CitreaAddress,
        recovery_taproot_address: &BitcoinAddress,
        config: &BridgeCliConfig,
    ) -> Result<(BitcoinAddress, bitcoin::taproot::TaprootSpendInfo), BridgeCliError> {
        crate::bitcoin_utils::calculate_deposit_address(
            citrea_address,
            recovery_taproot_address,
            config,
        )
    }

    // API utilities test helpers
    use bitcoin::{Block, Transaction, Txid};

    #[doc(hidden)]
    pub async fn test_get_block_height_for_tx(
        txid: &Txid,
        config: &BridgeCliConfig,
    ) -> Result<u64, BridgeCliError> {
        crate::api_utils::get_block_height_for_tx(txid, config).await
    }

    #[doc(hidden)]
    pub async fn test_get_tx_details(
        txid: &Txid,
        config: &BridgeCliConfig,
    ) -> Result<(Transaction, Block, u32), BridgeCliError> {
        crate::api_utils::get_tx_details(txid, config).await
    }

    #[doc(hidden)]
    pub async fn test_broadcast_recovery_tx(
        config: &BridgeCliConfig,
        raw_tx: String,
    ) -> Result<Txid, BridgeCliError> {
        crate::api_utils::broadcast_recovery_tx(config, raw_tx).await
    }

    // Deposit test helpers
    #[doc(hidden)]
    pub fn test_create_signed_recovery_tx(
        params: crate::deposit::RecoveryTxParams,
        config: &BridgeCliConfig,
        keypair: SecureKeypair,
    ) -> Result<Transaction, BridgeCliError> {
        crate::deposit::create_signed_recovery_tx(params, config, keypair)
    }

    // Withdrawal test helpers
    use bitcoin::Amount;
    use bitcoin::taproot::Signature;

    #[doc(hidden)]
    pub fn test_generate_withdrawal_signatures(
        keypair: SecureKeypair,
        signer_address: &crate::structs::TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
        destination_address: &BitcoinAddress,
        withdrawal_utxo: &OutPoint,
        optimistic_withdrawal_amount: &Amount,
        operator_withdrawal_amount: &Amount,
        config: &BridgeCliConfig,
    ) -> Result<(Signature, Signature), BridgeCliError> {
        crate::withdraw::generate_withdrawal_signatures(
            keypair,
            signer_address,
            destination_address,
            withdrawal_utxo,
            optimistic_withdrawal_amount,
            operator_withdrawal_amount,
            config,
        )
    }

    // Wallet test helpers
    #[doc(hidden)]
    pub fn test_get_private_key_from_wallet<T>(
        address: &crate::structs::TaprootAddressWithPrefix<T>,
        passphrase: &SecureString,
    ) -> Result<SecureSecretKey, BridgeCliError>
    where
        T: bitcoin::address::NetworkValidation,
        bitcoin::Address<T>: crate::structs::AddrDisplay,
    {
        crate::wallet::get_private_key_from_wallet(address, passphrase)
    }

    #[doc(hidden)]
    pub fn test_calculate_taproot_address(
        keypair: &SecureKeypair,
        network: bitcoin::Network,
    ) -> BitcoinAddress {
        crate::wallet::address::calculate_taproot_address(keypair, network)
    }

    // Wallet registry test helpers
    use std::collections::HashMap;

    /// Public test-only wrapper for wallet registry entry
    #[doc(hidden)]
    #[derive(Debug, Clone)]
    pub struct TestWalletRegistryEntry {
        pub label: String,
        pub network: String,
        pub created_at: String,
        pub addres_with_prefix: String,
        pub imported: Option<bool>,
        pub imported_at: Option<String>,
        pub import_method: Option<String>,
    }

    #[doc(hidden)]
    pub fn test_get_wallets_from_registry()
    -> Result<HashMap<String, TestWalletRegistryEntry>, BridgeCliError> {
        let registry = crate::wallet::wallet_storage::get_wallets_from_registry()?;
        Ok(registry
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    TestWalletRegistryEntry {
                        label: v.label,
                        network: v.network,
                        created_at: v.created_at,
                        addres_with_prefix: v.addres_with_prefix,
                        imported: v.imported,
                        imported_at: v.imported_at,
                        import_method: v.import_method,
                    },
                )
            })
            .collect())
    }

    // Common test helper functions

    #[doc(hidden)]
    pub fn setup_integration_test_env() {
        crate::clear_test_temp_dir();
    }

    #[doc(hidden)]
    pub fn test_passphrase() -> SecureString {
        SecureString::init_with(|| "test_passphrase".to_string())
    }

    #[doc(hidden)]
    pub fn wrong_passphrase() -> SecureString {
        SecureString::init_with(|| "wrong_passphrase".to_string())
    }

    #[doc(hidden)]
    pub fn test_network() -> bitcoin::Network {
        bitcoin::Network::Regtest
    }

    #[doc(hidden)]
    pub fn to_unchecked<T>(
        address: &crate::structs::TaprootAddressWithPrefix<T>,
    ) -> crate::structs::TaprootAddressWithPrefix<bitcoin::address::NetworkUnchecked>
    where
        T: bitcoin::address::NetworkValidation,
        bitcoin::Address<T>: crate::structs::AddrDisplay,
    {
        let addr_str = address.address_with_prefix();
        crate::structs::TaprootAddressWithPrefix::from_string_with_prefix_unchecked(&addr_str)
            .expect("Should convert to unchecked")
    }

    #[doc(hidden)]
    pub fn test_citrea_address() -> CitreaAddress {
        "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb1"
            .parse()
            .expect("Should parse test Citrea address")
    }

    #[doc(hidden)]
    pub fn test_citrea_address_2() -> CitreaAddress {
        "0x1234567890123456789012345678901234567890"
            .parse()
            .expect("Should parse test Citrea address")
    }

    #[doc(hidden)]
    pub fn test_outpoint_with_index(vout: u32) -> bitcoin::OutPoint {
        let txid: bitcoin::Txid =
            "1111111111111111111111111111111111111111111111111111111111111111"
                .parse()
                .expect("Should parse test txid");
        bitcoin::OutPoint::new(txid, vout)
    }

    #[doc(hidden)]
    pub fn test_private_key() -> SecureString {
        let sk_bytes = [0x11u8; 32]; // Fixed test key
        let secret_key =
            bitcoin::secp256k1::SecretKey::from_slice(&sk_bytes).expect("Should create secret key");
        SecureString::init_with(|| secret_key.display_secret().to_string())
    }

    #[doc(hidden)]
    pub fn test_private_keypair() -> SecureKeypair {
        let sk_bytes = [0x11u8; 32]; // Fixed test key
        let secret_key =
            bitcoin::secp256k1::SecretKey::from_slice(&sk_bytes).expect("Should create secret key");
        let keypair = bitcoin::secp256k1::Keypair::from_secret_key(&SECP, &secret_key);
        SecureKeypair::new(keypair)
    }

    #[doc(hidden)]
    pub fn test_private_keypair_2() -> SecureKeypair {
        let sk_bytes = [0x22u8; 32]; // Fixed test key
        let secret_key =
            bitcoin::secp256k1::SecretKey::from_slice(&sk_bytes).expect("Should create secret key");
        let keypair = bitcoin::secp256k1::Keypair::from_secret_key(&SECP, &secret_key);
        SecureKeypair::new(keypair)
    }

    #[doc(hidden)]
    pub fn test_secret_key_to_keypair(secret_key: &SecureSecretKey) -> SecureKeypair {
        let keypair =
            bitcoin::secp256k1::Keypair::from_secret_key(&SECP, secret_key.as_ref_inner());
        SecureKeypair::new(keypair)
    }

    #[doc(hidden)]
    pub fn test_invalid_private_key() -> SecureString {
        SecureString::init_with(|| "invalid_private_key".to_string())
    }

    #[doc(hidden)]
    pub fn test_short_private_key() -> SecureString {
        SecureString::init_with(|| "1234567890abcdef".to_string()) // 16 chars instead of 64
    }

    #[doc(hidden)]
    pub fn test_outpoint() -> bitcoin::OutPoint {
        test_outpoint_with_index(0)
    }

    #[doc(hidden)]
    pub fn test_fee_rate() -> bitcoin::FeeRate {
        bitcoin::FeeRate::from_sat_per_vb(10).expect("Should create fee rate")
    }

    #[doc(hidden)]
    pub fn test_amount() -> bitcoin::Amount {
        bitcoin::Amount::from_sat(100_000) // 0.001 BTC
    }

    #[doc(hidden)]
    pub fn test_large_amount() -> bitcoin::Amount {
        bitcoin::Amount::from_sat(1_000_000) // 0.01 BTC
    }

    #[doc(hidden)]
    pub fn test_config() -> BridgeCliConfig {
        BridgeCliConfig::defaults_for(test_network())
    }

    #[doc(hidden)]
    pub fn load_test_keypair<T>(
        address: &crate::structs::TaprootAddressWithPrefix<T>,
    ) -> SecureKeypair
    where
        T: bitcoin::address::NetworkValidation,
        bitcoin::Address<T>: crate::structs::AddrDisplay,
    {
        let passphrase = test_passphrase();
        let secret_key =
            get_private_key_from_wallet(address, &passphrase).expect("Should load private key");

        SecureKeypair::new(bitcoin::secp256k1::Keypair::from_secret_key(
            &SECP,
            secret_key.as_ref_inner(),
        ))
    }

    #[doc(hidden)]
    pub fn test_non_wallet_destination_address() -> BitcoinAddress {
        // Create a deterministic test key that won't be in the wallet
        let secret_bytes = [0x42u8; 32]; // Fixed test key
        let secret_key = bitcoin::secp256k1::SecretKey::from_slice(&secret_bytes)
            .expect("Should create secret key");
        let public_key = bitcoin::secp256k1::PublicKey::from_secret_key(&SECP, &secret_key);

        // Create a taproot address from the public key
        let tweaked_key = bitcoin::key::TweakedPublicKey::dangerous_assume_tweaked(
            public_key.x_only_public_key().0,
        );
        BitcoinAddress::p2tr_tweaked(tweaked_key, test_network())
    }

    #[doc(hidden)]
    pub fn test_load_key_with_purpose_check<T>(
        address: &TaprootAddressWithPrefix<T>,
        expected_purpose: Purpose,
    ) -> Result<SecureKeypair, BridgeCliError>
    where
        T: bitcoin::address::NetworkValidation,
        bitcoin::Address<T>: AddrDisplay,
    {
        load_key_with_purpose_check(address, expected_purpose)
    }
}
