use std::time::Duration;

use crate::core::errors::BridgeCliError;

pub mod api;
pub mod backend;

const HTTP_TIMEOUT_SECS: u64 = 20;

pub(crate) fn http_client() -> Result<reqwest::Client, BridgeCliError> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()?)
}

pub(crate) fn map_request_error(context: &str, e: reqwest::Error) -> BridgeCliError {
    tracing::error!("{}: {}", context, e);
    let message = if e.is_status() {
        match e.status() {
            Some(status) => format!(
                "{context}. Service returned HTTP status {status}; re-run with --verbose to see the details."
            ),
            None => format!(
                "{context}. Service returned an error status; re-run with --verbose to see the details."
            ),
        }
    } else if e.is_timeout() {
        format!("{context}. The request timed out; re-run with --verbose to see the details.")
    } else if e.is_connect() {
        format!(
            "{context}. This might be a connection issue; re-run with --verbose to see the details."
        )
    } else {
        format!("{context}. Re-run with --verbose to see the details.")
    };

    BridgeCliError::Eyre(eyre::eyre!(message))
}
