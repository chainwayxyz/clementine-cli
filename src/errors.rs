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

use crate::{config::ConfigErrors, storage::StorageError};
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

    // Module specific errors
    #[error("Can't get configuration: {0}")]
    ConfigError(ConfigErrors),
    #[error("Can't store/restore secret: {0}")]
    StorageError(#[from] StorageError),

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
