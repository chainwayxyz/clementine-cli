//! # Errors
//!
//! This module defines globally shared error messages, the crate-level error
//! wrapper and extension traits for error/results. Our error paradigm is as
//! follows:
//!
//! 1. Modules define their own error types when they need shared error messages.
//!    Module-level errors can wrap eyre::Report to capture arbitrary errors.
//! 2. The crate-level error wrapper (ClementineCliError) is used to wrap errors
//!    from modules and attach extra context (ie. which module caused the error).
//! 3. External crate errors are always wrapped by the ClementineCliError and
//!    never by module-level errors.
//! 4. When using external crates inside modules, extension traits are used to
//!    convert external-crate errors into ClementineCliError. This is further
//!    wrapped in an eyre::Report to avoid a circular dependency.
//! 5. ClementineCliError can be used to share error messages across modules.
//! 6. When the error cause is not sufficiently explained by the error messages,
//!    use `eyre::Context::wrap_err` to add more context. This will not hinder
//!    modules that are trying to match the error.

use crate::{BitcoinAddress, config::ConfigErrors, wallet::Purpose};
use bitcoin::Network;
use bitcoin::OutPoint;
use clap::builder::StyledStr;
use core::fmt::Debug;
use hex::FromHexError;
use thiserror::Error;

/// Errors returned by the Clementine CLI.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BridgeCliError {
    // Shared error messages
    #[error("Unsupported Bitcoin network: {0}")]
    UnsupportedNetwork(Network),

    // Address-related errors
    #[error("Failed to generate master seed from mnemonic: {0}")]
    MnemonicToSeedError(String),
    #[error("Address is not a taproot (P2TR) address: {0}")]
    NotTaprootAddress(String),
    #[error("Address field not found or invalid in wallet data")]
    MissingWalletAddress,
    #[error("Address already exists: {0}")]
    AddressAlreadyExists(String),
    #[error("The address must have a Purpose prefix, this is a plain address")]
    MissingPurposePrefix,
    #[error("Failed to get storage directory: {0}")]
    StorageDirectoryError(String),
    #[error("Failed to read storage directory: {0}")]
    StorageReadError(String),

    // Encryption-related errors
    #[error("Failed to generate random salt: {0}")]
    RandomSaltGenerationError(String),
    #[error("Failed to generate random nonce: {0}")]
    RandomNonceGenerationError(String),
    #[error("Key derivation failed: {0}")]
    KeyDerivationError(String),
    #[error("Encryption failed: {0}")]
    EncryptionError(String),
    #[error("Decryption failed: {0}")]
    DecryptionError(String),
    #[error("Decryption produced invalid UTF-8: {0}")]
    InvalidUtf8Error(String),
    #[error("Invalid nonce length: {0}")]
    InvalidNonceLength(usize),
    #[error("Invalid salt length: {0}")]
    InvalidSaltLength(usize),

    // Mnemonic-related errors
    #[error("Failed to generate mnemonic: {0}")]
    MnemonicGenerationError(String),
    #[error("Failed to parse mnemonic: {0}")]
    MnemonicParseError(String),
    #[error("No encrypted mnemonic found in wallet data")]
    MissingEncryptedMnemonic,
    #[error("Invalid mnemonic length: {0} words. Must be 12 words")]
    InvalidMnemonicLength(usize),
    #[error("Mnemonic validation failed: {0}")]
    MnemonicValidationFailed(String),
    #[error("Failed to encrypt placeholder mnemonic: {0}")]
    PlaceholderMnemonicEncryptionFailed(String),
    #[error("No mnemonic available - this wallet was imported from a private key")]
    NoMnemonicAvailable,
    #[error("Failed to display mnemonic securely: {0}")]
    FailedMnemonicDisplay(String),

    // Passphrase-related errors
    #[error("Invalid Argon2 parameters: {0}")]
    InvalidArgon2Parameters(String),
    #[error("Passphrase not provided for encrypted key")]
    PassphraseNotProvided,
    #[error("Invalid passphrase")]
    InvalidPassphrase,
    #[error("Passphrase is too short (minimum 8 characters)")]
    PassphraseTooShort,
    #[error("Passphrases do not match")]
    PassphraseMismatch,

    // Private key related errors
    #[error("No encrypted private key found in wallet data")]
    NoEncryptedPrivateKeyFound,

    // Wallet storage related errors
    #[error("Wallet with label '{0}' already exists")]
    LabelAlreadyExists(String),
    #[error("No wallet found with address: {0}")]
    WalletNotFound(String),
    #[error("Could not determine home directory")]
    HomeDirectoryNotFound,
    #[error("Wallets registry not found")]
    WalletsRegistryNotFound,

    // Wallet operation related errors
    #[error("Invalid address prefix: '{0}'. Valid prefixes are 'dep' and 'wit'.")]
    InvalidAddressPrefix(String),
    #[error("Invalid address format")]
    InvalidAddressFormat,
    #[error("Failed to generate address from mnemonic: {0}")]
    AddressGenerationFromMnemonicFailed(String),
    #[error("Failed to derive private key from mnemonic: {0}")]
    PrivateKeyDerivationFromMnemonicFailed(String),
    #[error("Failed to encrypt mnemonic: {0}")]
    MnemonicEncryptionFailed(String),
    #[error("Failed to encrypt private key: {0}")]
    PrivateKeyEncryptionFailed(String),
    #[error("Failed to store wallet: {0}")]
    WalletStorageFailed(String),
    #[error("Network mismatch: wallet is {0}, expected {1}")]
    NetworkMismatch(String, String),
    #[error("Invalid private key: {0}")]
    InvalidPrivateKey(String),
    #[error("Failed to parse wallets.json: {0}")]
    WalletsJsonParseFailed(String),
    #[error("Wallet file does not exist: {0}")]
    WalletFileNotFound(String),
    #[error("Path is not a file: {0}")]
    PathNotAFile(String),
    #[error("Invalid wallet file: missing network field")]
    MissingNetworkField,
    #[error("Invalid wallet file: missing encrypted_mnemonic field")]
    MissingEncryptedMnemonicField,
    #[error("Failed to parse encrypted private key structure: {0}")]
    EncryptedPrivateKeyParseError(String),
    #[error("Address mismatch! The decrypted key doesn't correspond to this wallet address.")]
    AddressMismatch,
    #[error(
        "Claim address cannot be a wallet address. The claim address belongs to one of your wallets."
    )]
    ClaimAddressIsWalletAddress,
    #[error("Address '{0}' should not have a prefix.")]
    AddressShouldNotHavePrefix(String),

    #[error("Invalid wallet file: missing encrypted_private_key field for private key import")]
    MissingEncryptedPrivateKeyField,

    #[error("Incorrect passphrase! Cannot decrypt wallet data.")]
    IncorrectPassphrase,

    // Deposit related errors
    #[error(
        "Calculated recovery taproot {0} not matches with Citrea response {1}: Please check configuration file and CLI version"
    )]
    CalculatedRecoveryTaprootAddressMismatch(BitcoinAddress, BitcoinAddress),

    #[error(
        "Can't broadcast raw transaction by either Mempool API or Bitcoin RPC: {mempool_api_error} and {bitcoin_rpc_error}"
    )]
    CantBroadcastTransaction {
        mempool_api_error: String,
        bitcoin_rpc_error: String,
    },

    #[error("Can't find the UTXO {0} in withdrawals")]
    CantFindUTXO(OutPoint),

    // Module specific errors
    #[error("Can't get configuration: {0}")]
    ConfigError(ConfigErrors),

    // External crate error wrappers
    #[error("Failed to convert hex string: {0}")]
    FromHexError(#[from] FromHexError),
    #[error("Failed to convert to hash from slice: {0}")]
    FromSliceError(#[from] bitcoin::hashes::FromSliceError),
    #[error("Error while calling EVM contract: {0}")]
    AlloyContract(#[from] alloy::contract::Error),
    #[error("Error while calling EVM RPC function: {0}")]
    AlloyRpc(#[from] alloy::transports::RpcError<alloy::transports::TransportErrorKind>),
    #[error("Error while encoding/decoding EVM type: {0}")]
    AlloySolTypes(#[from] alloy::sol_types::Error),
    #[error("{0}")]
    CLIDisplayAndExit(StyledStr),
    #[error("Can't make a request: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Can't serialize/deserialize data: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("{0}")]
    BitcoinRpcError(#[from] bitcoincore_rpc::Error),
    #[error("{0}")]
    BitcoinSecp256k1Error(#[from] bitcoin::secp256k1::Error),
    #[error("{0}")]
    BitcoinParseError(#[from] bitcoin::address::ParseError),
    #[error("{0}")]
    BitcoinHexParseError(#[from] bitcoin::hex::HexToArrayError),
    #[error("{0}")]
    BitcoinAmountParseError(#[from] bitcoin::amount::ParseAmountError),
    #[error("{0}")]
    BitcoinEncodeError(#[from] bitcoin::consensus::encode::Error),
    #[error("{0}")]
    BitcoinParseOutPointError(#[from] bitcoin::transaction::ParseOutPointError),
    #[error(
        "Wallet address purpose mismatch: expected {:?}, found {:?}. Please use {:?} wallet address(es) (addresses with \"{}\" prefix) for {:?} operations.",
        expected,
        found,
        expected,
        expected.to_prefix(),
        expected
    )]
    PurposeMismatch { expected: Purpose, found: Purpose },
    #[error(
        "Invalid prefix: {0}. Please make sure the address used has the correct purpose prefix (either \"dep\" or \"wit\")."
    )]
    InvalidPrefix(String),

    // IO errors (from rpassword and file operations)
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    // Base wrapper for eyre
    #[error(transparent)]
    Eyre(#[from] eyre::Report),
}

/// Extension traits for errors to easily convert them to [`eyre::Report`]  
/// through [`ClementineCliError`].
pub trait ErrorExt: Sized {
    /// Converts the error into an [`eyre::Report`], first wrapping in
    /// [`ClementineCliError`] if necessary. It does not rewrap in
    /// [`eyre::Report`] if the given error is already an [`eyre::Report`].
    fn into_eyre(self) -> eyre::Report;
}

/// Extension traits for results to easily convert them to [`eyre::Report`] and
/// through [`ClementineCliError`].
pub trait ResultExt: Sized {
    type Output;

    fn map_to_eyre(self) -> Result<Self::Output, eyre::Report>;
}

/// Extension for printing errors on any Result.
pub trait PrintErr {
    fn print_err(self) -> Self;
}

impl<T, E: std::fmt::Display> PrintErr for Result<T, E> {
    fn print_err(self) -> Self {
        if let Err(e) = &self {
            eprintln!("{}", e);
        }
        self
    }
}

impl<T: Into<BridgeCliError>> ErrorExt for T {
    fn into_eyre(self) -> eyre::Report {
        match self.into() {
            BridgeCliError::Eyre(report) => report,
            other => eyre::eyre!(other),
        }
    }
}

impl<U: Sized, T: Into<BridgeCliError>> ResultExt for Result<U, T> {
    type Output = U;

    fn map_to_eyre(self) -> Result<Self::Output, eyre::Report> {
        self.map_err(ErrorExt::into_eyre)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_downcast() {
        assert_eq!(
            BridgeCliError::UnsupportedNetwork(Network::Testnet)
                .into_eyre()
                .wrap_err("Some other error")
                .into_eyre()
                .wrap_err("some other")
                .downcast_ref::<BridgeCliError>()
                .unwrap()
                .to_string(),
            BridgeCliError::UnsupportedNetwork(Network::Testnet).to_string()
        );
    }
}
