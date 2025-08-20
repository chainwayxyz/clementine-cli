// Backend communication logic for Clementine CLI

use crate::config::BridgeCliConfig;
use crate::deposit::parse_taproot_address;
use crate::errors::BridgeCliError;
use crate::{BitcoinAddress, CitreaAddress};
use colored::*;
use serde_json::json;

/// Make a POST request to create a deposit account
pub fn create_deposit_account(
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<BitcoinAddress, BridgeCliError> {
    let url = format!("{}deposit-accounts", config.citrea_backend_endpoint);

    // Prepare request body
    let request_body = json!({
        "citrea_addr": citrea_address.to_string(),
        "recovery_taproot_addr": recovery_taproot_address.to_string()
    });

    tracing::debug!("Making request to: {}", url);
    tracing::debug!(
        "Request body: {}",
        serde_json::to_string_pretty(&request_body)?
    );

    // Create HTTP client
    let client = reqwest::blocking::Client::new();

    // Make POST request
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()?;

    if response.status().is_success() {
        let response_body: serde_json::Value = response.json()?;
        tracing::info!(
            "{} Deposit address request successful",
            "SUCCESS".green().bold(),
        );
        tracing::debug!(
            "Response: {}",
            serde_json::to_string_pretty(&response_body)?
        );
        // parse the json and get the taproot_addr and parse it to an address
        let taproot_addr = response_body["taproot_addr"].as_str().unwrap();
        let taproot_addr = parse_taproot_address(taproot_addr, config.network)?;

        Ok(taproot_addr)
    } else {
        let status = response.status();
        let error_text = response.text()?;
        println!("{} Deposit address request failed", "ERROR".red().bold());
        println!("{} {}", "STATUS".red().bold(), status);
        tracing::debug!("Error response: {}", error_text);

        Err(eyre::eyre!(
            "Backend request failed with status: {} {}",
            status,
            error_text
        )
        .into())
    }
}
