//! # Errors
//!
//! This module defines globally shared error messages, the crate-level error
//! wrapper and extension traits for error/results. Our error paradigm is as
//! follows:
//!
//! 1. Modules define their own error types when they need shared error messages.
//!    Module-level errors can wrap eyre::Report to capture arbitrary errors.
//! 2. The crate-level error wrapper (BridgeCliError) is used to wrap errors
//!    from modules and attach extra context (ie. which module caused the error).
//! 3. External crate errors are always wrapped by the BridgeCliError and
//!    never by module-level errors.
//! 4. When using external crates inside modules, extension traits are used to
//!    convert external-crate errors into BridgeCliError. This is further
//!    wrapped in an eyre::Report to avoid a circular dependency.
//! 5. BridgeCliError can be used to share error messages across modules.
//! 6. When the error cause is not sufficiently explained by the error messages,
//!    use `eyre::Context::wrap_err` to add more context. This will not hinder
//!    modules that are trying to match the error.

use crate::{BitcoinAddress, config::ConfigErrors, wallet::Purpose};
use bitcoin::Network;
use bitcoin::OutPoint;
use bitcoin::address::ParseError;
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
    #[error("Failed to generate random salt.")]
    RandomSaltGenerationError,
    #[error("Failed to generate random nonce.")]
    RandomNonceGenerationError,
    #[error("Key derivation failed.")]
    EncryptionKeyDerivationError,
    #[error("Encryption failed.")]
    EncryptionError,
    #[error("Decryption failed.")]
    DecryptionError,
    #[error("Decryption produced invalid UTF-8.")]
    InvalidUtf8Error,
    #[error("Invalid nonce length: {0}")]
    InvalidNonceLength(usize),
    #[error("Invalid salt length: {0}")]
    InvalidSaltLength(usize),

    // Mnemonic-related errors
    #[error("Failed to generate mnemonic.")]
    MnemonicGenerationError,
    #[error("Failed to parse mnemonic.")]
    MnemonicParseError,
    #[error("No encrypted mnemonic found in wallet data")]
    MissingEncryptedMnemonic,
    #[error("Invalid mnemonic length: {0} words. Must be 12 words")]
    InvalidMnemonicLength(usize),
    #[error("Mnemonic validation failed.")]
    MnemonicValidationFailed,
    #[error("Failed to encrypt placeholder mnemonic.")]
    PlaceholderMnemonicEncryptionFailed,
    #[error("No mnemonic available - this wallet was imported from a private key")]
    NoMnemonicAvailable,
    #[error("Failed to display mnemonic securely.")]
    FailedMnemonicDisplay,

    // Passphrase-related errors
    #[error("Invalid Argon2 parameters.")]
    InvalidArgon2Parameters,
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
    #[error("Invalid address format: {0}")]
    InvalidAddressFormat(String),
    #[error("Failed to generate address from mnemonic.")]
    AddressGenerationFromMnemonicFailed,
    #[error("Failed to derive private key from mnemonic.")]
    PrivateKeyDerivationFromMnemonicFailed,
    #[error("Failed to encrypt mnemonic.")]
    MnemonicEncryptionFailed,
    #[error("Failed to encrypt private key.")]
    PrivateKeyEncryptionFailed,
    #[error("Failed to store wallet.")]
    WalletStorageFailed,
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
    #[error("Address mismatch! The decrypted key doesn't correspond to this wallet address.")]
    AddressMismatch,
    #[error(
        "Destination address cannot be a wallet address. The destination address belongs to one of your wallets."
    )]
    DestinationAddressIsWalletAddress,
    #[error(
        "Address '{0}' should not have a prefix, do not use a Clementine wallet for this operation."
    )]
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

    #[error("Transaction {0} is not found on chain, maybe wait for confirmation")]
    TransactionNotOnChain(bitcoin::Txid),

    // Transaction parsing errors
    #[error("Failed to decode hex string '{hex_string}': {source}")]
    HexDecodeError {
        source: hex::FromHexError,
        hex_string: String,
    },
    #[error("Failed to deserialize transaction from hex '{tx_hex}': {source}")]
    TransactionDeserializeError {
        source: bitcoin::consensus::encode::Error,
        tx_hex: String,
    },

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
    BitcoinParseError(String),
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
/// through [`BridgeCliError`].
pub trait ErrorExt: Sized {
    /// Converts the error into an [`eyre::Report`], first wrapping in
    /// [`BridgeCliError`] if necessary. It does not rewrap in
    /// [`eyre::Report`] if the given error is already an [`eyre::Report`].
    fn into_eyre(self) -> eyre::Report;
}

/// Extension traits for results to easily convert them to [`eyre::Report`] and
/// through [`BridgeCliError`].
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

impl From<bitcoin::address::ParseError> for BridgeCliError {
    fn from(err: bitcoin::address::ParseError) -> Self {
        match err {
            ParseError::NetworkValidation(_) => {
                Self::BitcoinParseError("Address network doesn't match expected network. You might have forgotten to specify the network. Please check your configuration and your address.".to_string())
            },
            // For other variants, use the default error message
            _ => Self::BitcoinParseError(err.to_string())
        }
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
