use crate::config::BridgeCliConfig;
use crate::deposit::DepositStatusEnum;
use crate::errors::BridgeCliError;
use crate::wallet::address::parse_taproot_address;
use crate::withdraw::WithdrawStatusEnum;
use crate::{BitcoinAddress, CitreaAddress};
use bitcoin::{Address, OutPoint};
use colored::*;
use eyre::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::borrow::Cow;
use std::fmt::Display;

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

pub struct DepositStatusWithVout<'a> {
    pub deposit_status: &'a DepositStatus,
    pub vout: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WithdrawStatus {
    pub idx: u64,
    pub status: String,
    pub btc_payment_txid: String,
    pub from_safe_withdraw: bool,
    pub optimistic_payout_started_at: Option<String>,
    pub optimistic_payout_deadline_at: Option<String>,
    pub created_at: String,
}

#[allow(clippy::too_many_arguments)]
fn format_deposit_status(
    f: &mut std::fmt::Formatter<'_>,
    id: u64,
    status: &str,
    txid: &str,
    vout: Option<u32>,
    evm_addr: &str,
    move_txid: &str,
    mint_txid: &str,
    print_na_for_vout: bool,
) -> std::fmt::Result {
    let display_or = |v: &str| {
        if v.is_empty() {
            "--".to_string()
        } else {
            v.to_string()
        }
    };

    writeln!(f, "\nDeposit Info")?;
    writeln!(f, "  ID:            {}", id)?;
    writeln!(f, "  Status:        {}", status)?;
    writeln!(f, "  TXID:          {}", display_or(txid))?;
    match vout {
        Some(v) => writeln!(f, "  UTXO Outpoint: {}:{}", txid, v)?,
        None => {
            if print_na_for_vout {
                writeln!(f, "  UTXO Outpoint: N/A")?
            }
        }
    }
    writeln!(f, "  EVM Addr:      {}", display_or(evm_addr))?;
    writeln!(f, "  Move TXID:     {}", display_or(move_txid))?;
    writeln!(f, "  Mint TXID:     {}", display_or(mint_txid))
}

// Overload for default print_na_for_vout = false
fn format_deposit_status_default(
    f: &mut std::fmt::Formatter<'_>,
    id: u64,
    status: &str,
    txid: &str,
    evm_addr: &str,
    move_txid: &str,
    mint_txid: &str,
) -> std::fmt::Result {
    format_deposit_status(
        f, id, status, txid, None, evm_addr, move_txid, mint_txid, false,
    )
}
impl Display for DepositStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.status.is_empty() {
            Cow::Borrowed("--")
        } else {
            Cow::Owned(DepositStatusEnum::from_status(&self.status).as_string())
        };
        format_deposit_status_default(
            f,
            self.id,
            &status,
            &self.txid,
            &self.evm_addr,
            &self.move_txid,
            &self.mint_txid,
        )
    }
}

impl Display for DepositStatusWithVout<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.deposit_status.status.is_empty() {
            Cow::Borrowed("--")
        } else {
            Cow::Owned(DepositStatusEnum::from_status(&self.deposit_status.status).as_string())
        };
        format_deposit_status(
            f,
            self.deposit_status.id,
            &status,
            &self.deposit_status.txid,
            self.vout,
            &self.deposit_status.evm_addr,
            &self.deposit_status.move_txid,
            &self.deposit_status.mint_txid,
            true,
        )
    }
}

impl Display for WithdrawStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn display_or(v: &str) -> &str {
            if v.is_empty() { "--" } else { v }
        }

        fn display_option_or<T: ToString>(v: &Option<T>) -> String {
            if v.is_none() {
                "--".to_string()
            } else {
                v.as_ref().unwrap().to_string()
            }
        }

        fn display_t<T: ToString + Display>(v: &T) -> String {
            v.to_string()
        }

        let status = if self.status.is_empty() {
            Cow::Borrowed("--")
        } else {
            Cow::Owned(WithdrawStatusEnum::from_backend_status(&self.status).as_string())
        };

        write!(
            f,
            "\nWithdrawal Info\n  Index:                {}\n  Status:               {}\n  BTC Payment TXID:     {}\n  From Safe Withdraw:   {}\n  Payout Started:       {}\n  Payout Deadline:      {}\n  Created:              {}",
            self.idx,
            status,
            display_or(&self.btc_payment_txid),
            display_t(&self.from_safe_withdraw),
            display_option_or(&self.optimistic_payout_started_at),
            display_option_or(&self.optimistic_payout_deadline_at),
            display_or(&self.created_at)
        )
    }
}

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

pub(crate) async fn backend_withdrawal_status(
    withdrawal_index: u32,
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
        .query(&[("idx", withdrawal_index.to_string())])
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

pub(crate) async fn send_withdrawal_signature_to_operators(
    _signer_address: &str,
    withdrawal_address: &str,
    withdrawal_outpoint: OutPoint,
    withdrawal_index: u32,
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

        Err(eyre::eyre!(
            "Backend request failed with status: {} {}",
            status,
            error_text
        )
        .into())
    }
}
