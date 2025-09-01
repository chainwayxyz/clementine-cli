use std::fmt::Display;

use serde::{Deserialize, Serialize};
#[derive(Debug, Deserialize, Serialize)]
pub struct DepositStatus {
    pub id: u64,
    pub status: String,
    pub txid: String,
    pub evm_addr: String,
    pub move_txid: String,
    pub created_at: String,
    pub mint_txid: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WithdrawalStatus {
    pub idx: u64,
    pub status: String,
    pub btc_payment_txid: String,
    pub from_safe_withdraw: bool,
    pub optimistic_payout_started_at: Option<String>,
    pub optimistic_payout_deadline_at: Option<String>,
    pub created_at: String,
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

impl Display for WithdrawalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let not_present = "--";
        let status = if self.status.is_empty() {
            not_present
        } else {
            &self.status
        };
        let btc_payment_txid = if self.btc_payment_txid.is_empty() {
            not_present
        } else {
            &self.btc_payment_txid
        };
        let optimistic_payout_started_at = self
            .optimistic_payout_started_at
            .as_deref()
            .unwrap_or(not_present);
        let optimistic_payout_deadline_at = self
            .optimistic_payout_deadline_at
            .as_deref()
            .unwrap_or(not_present);
        write!(
            f,
            "Withdrawal Index: {}, Status: {}, BTC Payment TXID: {}, From Safe Withdraw: {}, Payout Started: {}, Payout Deadline: {}, Created: {}",
            self.idx,
            status,
            btc_payment_txid,
            self.from_safe_withdraw,
            optimistic_payout_started_at,
            optimistic_payout_deadline_at,
            self.created_at
        )
    }
}
// Backend communication logic for Clementine CLI

use crate::config::BridgeCliConfig;
use crate::errors::BridgeCliError;
use crate::wallet::address::parse_taproot_address;
use crate::{BitcoinAddress, CitreaAddress};
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

pub(crate) async fn backend_withdrawal_status(
    withdrawal_index: u32,
    config: &BridgeCliConfig,
) -> Result<Vec<WithdrawalStatus>, BridgeCliError> {
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
        .query(&[("idx", withdrawal_index.to_string())])
        .send()
        .await?;

    if response.status().is_success() {
        let response_body: Vec<WithdrawalStatus> = response.json().await?;
        tracing::info!(
            "{} Withdrawal status request successful",
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

pub(crate) async fn send_withdrawal_signatures_to_operators(
    _signer_address: &str,
    withdrawal_address: &str,
    withdrawal_outpoint: OutPoint,
    withdrawal_index: u32,
    signature: &str,
    config: &BridgeCliConfig,
    amount: f64,
) -> Result<(), BridgeCliError> {
    let url = config
        .citrea_backend_endpoint // As long as URL is only base, no trailing / (slash) is needed
        .join("withdrawals/user-signatures")
        .wrap_err("Can't join endpoint with the URL")?;

    // Prepare request body
    let request_body = json!({
        "withdrawal_idx": withdrawal_index,
        "signature": signature,
        "input_outpoint": withdrawal_outpoint.to_string(),
        "output_script_pubkey": withdrawal_address,
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
            "SUCCESS".green().bold(),
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

        Err(eyre::eyre!(
            "Backend request failed with status: {} {}",
            status,
            error_text
        )
        .into())
    }
}
