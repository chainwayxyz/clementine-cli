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

use crate::config::ConfigErrors;
use clap::builder::StyledStr;
use core::fmt::Debug;
use hex::FromHexError;
use thiserror::Error;

/// Errors returned by the Clementine CLI.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BridgeCliError {
    // Shared error messages
    #[error("Unsupported network")]
    UnsupportedNetwork,
    
    // Address-related errors
    #[error("Failed to generate master seed from mnemonic")]
    MnemonicToSeedError,
    #[error("Invalid Bitcoin address format")]
    InvalidAddressFormat,
    #[error("Address is not a taproot (P2TR) address")]
    NotTaprootAddress,
    #[error("Address field not found or invalid in wallet data")]
    MissingWalletAddress,
    #[error("Failed to get storage directory")]
    StorageDirectoryError,
    #[error("Failed to read storage directory")]
    StorageReadError,
    
    // Encryption-related errors
    #[error("Failed to generate random salt")]
    RandomSaltGenerationError,
    #[error("Failed to generate random nonce")]
    RandomNonceGenerationError,
    #[error("Key derivation failed")]
    KeyDerivationError,
    #[error("Encryption failed")]
    EncryptionError,
    #[error("Decryption failed: Incorrect passphrase")]
    DecryptionError,
    #[error("Decryption produced invalid UTF-8")]
    InvalidUtf8Error,
    #[error("Invalid nonce length")]
    InvalidNonceLength,
    #[error("Invalid salt length")]
    InvalidSaltLength,
    
    // Mnemonic-related errors
    #[error("Failed to generate mnemonic")]
    MnemonicGenerationError,
    #[error("Failed to parse mnemonic")]
    MnemonicParseError,
    #[error("No encrypted mnemonic found in wallet data")]
    MissingEncryptedMnemonic,
    #[error("Invalid mnemonic length: {0} words. Must be 12 words")]
    InvalidMnemonicLength(usize),
    #[error("Mnemonic validation failed: {0}")]
    MnemonicValidationFailed(String),
    #[error("Failed to encrypt placeholder mnemonic")]
    PlaceholderMnemonicEncryptionFailed,
    
        // Passphrase-related errors
    #[error("Invalid Argon2 parameters")]
    InvalidArgon2Parameters,
    #[error("Passphrase not provided for encrypted key")]
    PassphraseNotProvided,
    #[error("Invalid passphrase")]
    InvalidPassphrase,
    #[error("Passphrase cannot be empty")]
    EmptyPassphrase,
    #[error("Passphrase is too short (minimum 8 characters)")]
    PassphraseTooShort,
    #[error("Passphrases do not match")]
    PassphraseMismatch,
    
    // Private key related errors
    #[error("No encrypted private key found in wallet data")]
    NoEncryptedPrivateKeyFound,
    
    // Wallet storage related errors
    #[error("Wallet with address '{0}' already exists")]
    WalletAlreadyExists(String),
    #[error("No wallet found with address: {0}")]
    WalletNotFound(String),
    #[error("Could not determine home directory")]
    HomeDirectoryNotFound,
    
    // Wallet operation related errors
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
    #[error("Maximum attempts exceeded. Operation cancelled for security.")]
    MaxAttemptsExceeded,
    #[error("Maximum attempts exceeded for passphrase confirmation.")]
    PassphraseConfirmationMaxAttemptsExceeded,
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


    #[error("Invalid wallet file: missing encrypted_private_key field for private key import")]
    MissingEncryptedPrivateKeyField,


    #[error("Incorrect passphrase! Cannot decrypt wallet data.")]
    IncorrectPassphrase,

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
    BitcoinParseOutpiontError(#[from] bitcoin::transaction::ParseOutPointError),

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
            BridgeCliError::UnsupportedNetwork
                .into_eyre()
                .wrap_err("Some other error")
                .into_eyre()
                .wrap_err("some other")
                .downcast_ref::<BridgeCliError>()
                .unwrap()
                .to_string(),
            BridgeCliError::UnsupportedNetwork.to_string()
        );
    }
}
