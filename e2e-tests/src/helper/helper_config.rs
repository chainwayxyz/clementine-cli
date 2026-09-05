use anyhow::{Result, anyhow};
use bitcoin::{Amount, Network, XOnlyPublicKey};
use citrea_e2e::config::BitcoinConfig as E2eBitcoinConfig;
use clementine_cli::config::{BitcoinConfig, BridgeCliConfig};
use reqwest::Url;
use std::str::FromStr;

/// Build a BridgeCliConfig for regtest using Bitcoin RPC connection details.
pub fn regtest_bridge_cli_config_with_rpc(
    rpc_host: &str,
    rpc_port: u16,
    rpc_user: &str,
    rpc_password: &str,
) -> Result<BridgeCliConfig> {
    let rpc_url = Url::parse(&format!("http://{}:{}/", rpc_host, rpc_port))
        .map_err(|e| anyhow!("Invalid Bitcoin RPC URL: {}", e))?;

    let bitcoin_config = BitcoinConfig {
        url: rpc_url,
        user: rpc_user.to_string(),
        password: rpc_password.to_string(),
    };

    Ok(BridgeCliConfig {
        network: Network::Regtest,
        aggregated_public_key: XOnlyPublicKey::from_str(
            "30ff95ec2726938072a2009f3276cd8fba2363d9284a7eb01217b2f302eb8577",
        )
        .unwrap(),
        esplora_rest_api: None,
        citrea_chain_id: 5655,
        citrea_rpc_url: None,
        citrea_backend_endpoint: Url::parse("https://127.0.0.1/").unwrap(),
        withdrawal_sign_url: Url::parse("http://127.0.0.1:12345/").unwrap(),
        user_takes_after: 200,
        bridge_amount: Amount::from_sat(1_000_000_000),
        optimistic_withdrawal_amount: Amount::from_sat(999_999_760),
        operator_withdrawal_amount: Amount::from_sat(997_000_000),
        dust_utxo_amount: Amount::from_sat(330),
        bridge_contract_address: "0x3100000000000000000000000000000000000002".into(),
        move_tx_finalization_blocks: 5,
        bitcoin_config: Some(bitcoin_config),
    })
}

/// Build a BridgeCliConfig for regtest from the e2e Bitcoin node configuration.
pub fn regtest_bridge_cli_config_from_bitcoin_config(
    bitcoin_config: &E2eBitcoinConfig,
) -> Result<BridgeCliConfig> {
    let rpc_host = "127.0.0.1";

    regtest_bridge_cli_config_with_rpc(
        rpc_host,
        bitcoin_config.rpc_port,
        &bitcoin_config.rpc_user,
        &bitcoin_config.rpc_password,
    )
}
