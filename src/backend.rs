use std::fmt::Display;

use serde::Deserialize;
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct DepositStatus {
    pub id: u64,
    pub status: String,
    pub txid: String,
    pub evm_addr: String,
    pub move_txid: String,
    pub created_at: String,
    pub mint_txid: String,
}

impl Display for DepositStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let not_present = "--";
        let status = if self.status.is_empty() {
            not_present
        } else {
            &self.status
        };
        let txid = if self.txid.is_empty() {
            not_present
        } else {
            &self.txid
        };
        let evm_addr = if self.evm_addr.is_empty() {
            not_present
        } else {
            &self.evm_addr
        };
        let move_txid = if self.move_txid.is_empty() {
            not_present
        } else {
            &self.move_txid
        };
        let mint_txid = if self.mint_txid.is_empty() {
            not_present
        } else {
            &self.mint_txid
        };
        write!(
            f,
            "Deposit ID: {}, Status: {}, TXID: {}, EVM Address: {}, Move TXID: {}, Mint TXID: {}",
            self.id, status, txid, evm_addr, move_txid, mint_txid
        )
    }
}
// Backend communication logic for Clementine CLI

use crate::config::BridgeCliConfig;
use crate::errors::BridgeCliError;
use crate::wallet::address::parse_taproot_address;
use crate::{BitcoinAddress, CitreaAddress};
use bitcoin::Address;
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

pub(crate) async fn backend_deposit_status(
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

    // Make POST request
    let response = client
        .get(url.as_str())
        .header("Content-Type", "application/json")
        .query(&[("taproot_addrs", taproot_address.to_string())])
        .send()
        .await?;

    if response.status().is_success() {
        let response_body: Vec<DepositStatus> = response.json().await?;
        tracing::info!(
            "{} Deposit status request successful",
            "SUCCESS".green().bold(),
        );
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
