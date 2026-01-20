use crate::core::config::BridgeCliConfig;
use crate::core::errors::BridgeCliError;
use crate::deposit::CitreaAddress;
use crate::deposit::DepositStatus;
use crate::wallet::BitcoinAddress;
use crate::wallet::address::parse_taproot_address;
use crate::withdraw::WithdrawStatus;
use bitcoin::{Address, OutPoint};
use colored::*;
use eyre::{Context, Result};
use serde_json::json;

/// Make a POST request to create a deposit account
pub(crate) async fn create_deposit_account(
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<BitcoinAddress, BridgeCliError> {
    let url = config
        .citrea_backend_endpoint // As long as URL is only base, no trailing / (slash) is needed
        .join("deposit-accounts")
        .wrap_err("Can't join endpoint with the URL")?;

    // Prepare request body
    let request_body = json!({
        "evm_addr": citrea_address.to_string(),
        "recovery_taproot_addr": recovery_taproot_address.to_string()
    });

    tracing::debug!("Making request to: {}", url);
    tracing::debug!(
        "Request body: {}",
        serde_json::to_string_pretty(&request_body)?
    );

    // Create HTTP client
    let client = reqwest::Client::new();

    // Make POST request
    let response = client
        .post(url.as_str())
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    if response.status().is_success() {
        let response_body: serde_json::Value = response.json().await?;
        tracing::info!("{} Deposit address request successful", "SUCCESS".bold(),);
        tracing::debug!(
            "Response: {}",
            serde_json::to_string_pretty(&response_body)?
        );
        // parse the json and get the taproot_addr and parse it to an address
        let taproot_addr = response_body
            .get("taproot_addr")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                BridgeCliError::Eyre(eyre::eyre!("Backend response missing taproot_addr"))
            })?;
        let taproot_addr = parse_taproot_address(taproot_addr, config.network)?;

        Ok(taproot_addr)
    } else {
        let status = response.status();
        let error_text = response.text().await?;
        tracing::error!("Deposit address request failed: {}", status);
        tracing::error!("Error response: {}", error_text);

        Err(eyre::eyre!(
            "Backend request failed with status: {} {}",
            status,
            error_text
        )
        .into())
    }
}

pub async fn backend_deposit_status(
    taproot_address: &Address,
    config: &BridgeCliConfig,
) -> Result<Vec<DepositStatus>, BridgeCliError> {
    let url = config
        .citrea_backend_endpoint // As long as URL is only base, no trailing / (slash) is needed
        .join("deposits")
        .wrap_err("Can't join endpoint with the URL")?;

    tracing::debug!("Making request to: {}", url);

    // Create HTTP client
    let client = reqwest::Client::new();

    // Make GET request
    let response = client
        .get(url.as_str())
        .header("Content-Type", "application/json")
        .query(&[("taproot_addrs", taproot_address.to_string())])
        .send()
        .await?;

    if response.status().is_success() {
        let response_body: Vec<DepositStatus> = response.json().await?;
        tracing::info!("{} Deposit status request successful", "SUCCESS".bold(),);
        tracing::debug!(
            "Response: {}",
            serde_json::to_string_pretty(&response_body)?
        );
        Ok(response_body)
    } else {
        let status = response.status();
        let error_text = response.text().await?;
        tracing::error!("Deposit status request failed: {}", status);
        tracing::error!("Error response: {}", error_text);

        Err(eyre::eyre!(
            "Backend request failed with status: {} {}",
            status,
            error_text
        )
        .into())
    }
}

pub async fn backend_withdrawal_status(
    withdrawal_outpoint: OutPoint,
    config: &BridgeCliConfig,
) -> Result<Vec<WithdrawStatus>, BridgeCliError> {
    let url = config
        .citrea_backend_endpoint // As long as URL is only base, no trailing / (slash) is needed
        .join("withdrawals")
        .wrap_err("Can't join endpoint with the URL")?;

    tracing::debug!("Making request to: {}", url);

    // Create HTTP client
    let client = reqwest::Client::new();

    // Make GET request
    let response = client
        .get(url.as_str())
        .header("Content-Type", "application/json")
        .query(&[("user_dust_outpoint", withdrawal_outpoint.to_string())])
        .send()
        .await?;

    if response.status().is_success() {
        let response_body: Vec<WithdrawStatus> = response.json().await?;
        tracing::info!("{} Withdrawal status request successful", "SUCCESS".bold(),);
        tracing::debug!(
            "Response: {}",
            serde_json::to_string_pretty(&response_body)?
        );
        Ok(response_body)
    } else {
        let status = response.status();
        let error_text = response.text().await?;
        tracing::error!("Withdrawal status request failed: {}", status);
        tracing::error!("Error response: {}", error_text);

        Err(eyre::eyre!(
            "Backend request failed with status: {} {}",
            status,
            error_text
        )
        .into())
    }
}

pub async fn send_withdrawal_signature_to_operators(
    _signer_address: &str,
    destination_address: &str,
    withdrawal_outpoint: OutPoint,
    signature: &str,
    config: &BridgeCliConfig,
    amount: u64,
) -> Result<(), BridgeCliError> {
    let url = config
        .citrea_backend_endpoint // As long as URL is only base, no trailing / (slash) is needed
        .join("withdrawals/user-signatures")
        .wrap_err("Can't join endpoint with the URL")?;

    // Prepare request body
    let request_body = json!({
        "signature": signature,
        "user_dust_outpoint": withdrawal_outpoint.to_string(),
        "output_script_pubkey": destination_address,
        "output_amount": amount
    });

    tracing::debug!("Making request to: {}", url);
    tracing::debug!(
        "Request body: {}",
        serde_json::to_string_pretty(&request_body)?
    );

    // Create HTTP client
    let client = reqwest::Client::new();

    // Make POST request
    let response = client
        .post(url.as_str())
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    if response.status().is_success() {
        let response_body: serde_json::Value = response.json().await?;
        tracing::info!(
            "{} Withdrawal signatures sent successfully",
            "SUCCESS".bold(),
        );
        tracing::debug!(
            "Response: {}",
            serde_json::to_string_pretty(&response_body)?
        );
        Ok(())
    } else {
        let status = response.status();
        let error_text = response.text().await?;
        tracing::error!("Send withdrawal signatures request failed: {}", status);
        tracing::error!("Error response: {}", error_text);

        if error_text.contains("Withdrawal not found") {
            Err(eyre::eyre!(
                "Withdrawal not found for outpoint: {}, maybe wait for confirmation.",
                withdrawal_outpoint
            )
            .into())
        } else if error_text.contains("Withdrawal user signature already exists") {
            Err(eyre::eyre!(
                "Signature already submitted for withdrawal outpoint: {}",
                withdrawal_outpoint
            )
            .into())
        } else {
            Err(eyre::eyre!("Internal Error while sending withdrawal signature").into())
        }
    }
}
