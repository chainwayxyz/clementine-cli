// Backend communication logic for Clementine CLI

use crate::config::BridgeCliConfig;
use crate::errors::BridgeCliError;
use crate::wallet::address::parse_taproot_address;
use crate::{BitcoinAddress, CitreaAddress};
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

    // Request body for the deposit-accounts endpoint
    let request_body = json!({
        "evm_addr": citrea_address.to_string(),
        "recovery_taproot_addr": recovery_taproot_address.to_string()
    });

    tracing::debug!(
        "Making request to {} with request body {}",
        url,
        serde_json::to_string_pretty(&request_body)?
    );

    let response = reqwest::Client::new()
        .post(url.as_str())
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    if response.status().is_success() {
        let response_body: serde_json::Value = response.json().await?;
        tracing::info!(
            "Deposit address request successful: {}",
            serde_json::to_string_pretty(&response_body)?
        );

        let taproot_addr = response_body["taproot_addr"].as_str().unwrap();
        let taproot_addr = parse_taproot_address(taproot_addr, config.network)?;

        Ok(taproot_addr)
    } else {
        let status = response.status();
        let error_text = response.text().await?;

        Err(eyre::eyre!(
            "Backend request failed with status: {}: {}",
            status,
            error_text
        )
        .into())
    }
}
