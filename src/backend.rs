// Backend communication logic for Clementine CLI

use crate::config::CliConfig;
use crate::deposit::parse_taproot_address;
use crate::{BitcoinAddress, CitreaAddress};
use colored::*;
use reqwest::Url;
use serde_json::json;

/// Make a POST request to create a deposit account
pub fn create_deposit_account(
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &BitcoinAddress,
    config: &CliConfig,
) -> Result<BitcoinAddress, Box<dyn std::error::Error>> {
    let base_url = Url::parse(&config.citrea_backend_endpoint)?;
    let url = base_url.join("deposit-accounts")?;

    // Prepare request body
    let request_body = json!({
        "citrea_addr": citrea_address.to_string(),
        "recovery_taproot_addr": recovery_taproot_address.to_string()
    });

    debug!("Making request to: {}", url);
    debug!(
        "Request body: {}",
        serde_json::to_string_pretty(&request_body)?
    );

    // Create HTTP client
    let client = reqwest::blocking::Client::new();

    // Make POST request
    let response = client
        .post(url.as_str())
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()?;

    if response.status().is_success() {
        let response_body: serde_json::Value = response.json()?;
        println!(
            "{} Deposit address request successful",
            "SUCCESS".green().bold(),
        );
        debug!(
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
        debug!("Error response: {}", error_text);
        Err(format!(
            "Backend request failed with status: {} {}",
            status, error_text
        )
        .into())
    }
}
