//! # Configuration Options
//!
//! Configuration options provided here are used to make a request to Clementine.

use bitcoin::{Amount, Network, XOnlyPublicKey};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use eyre::Result;
use reqwest::Url;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::{fs::File, io::Read, path::PathBuf, str::FromStr, sync::LazyLock};
use thiserror::Error;

use crate::errors::BridgeCliError;

pub static UNSPENDABLE_XONLY_PUBKEY: LazyLock<XOnlyPublicKey> = LazyLock::new(|| {
    XOnlyPublicKey::from_str("93c7378d96518a75448821c4f7c8f4bae7ce60f804d03d1f0628dd5dd0f5de51")
        .unwrap()
});

#[derive(Debug, Error)]
pub enum ConfigErrors {
    #[error("Can't read configuration file: {0}")]
    FileReadFailure(#[from] std::io::Error),
    #[error("Can't parse TOML file: {0}")]
    TomlError(#[from] toml::de::Error),

    #[error(transparent)]
    Other(#[from] eyre::Report),
}

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeCliConfig {
    pub network: Network,
    pub aggregated_public_key: XOnlyPublicKey,
    pub mempool_api_url: Url,
    pub citrea_chain_id: u64,
    pub citrea_rpc_url: Url,
    pub citrea_backend_endpoint: Url,
    pub user_takes_after: u64,
    pub bridge_amount: Amount,
    pub bridge_contract_address: String,
    pub bitcoin_config: Option<BitcoinConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BitcoinConfig {
    pub url: Url,
    pub password: SecretString,
    pub user: SecretString,
}

impl BridgeCliConfig {
    pub fn new() -> Self {
        BridgeCliConfig::default()
    }

    /// Read contents of a TOML file and generate a [`CliConfig`].
    pub fn try_parse_file(path: PathBuf) -> Result<Self, ConfigErrors> {
        let mut contents = String::new();

        let mut file = File::open(path.clone())?;
        file.read_to_string(&mut contents)?;

        Self::try_parse_from(contents)
    }

    /// Try to parse a [`CliConfig`] from given TOML formatted string and
    /// generate a [`CliConfig`].
    pub fn try_parse_from(input: String) -> Result<Self, ConfigErrors> {
        Ok(toml::from_str::<Self>(&input)?)
    }

    pub async fn connect_to_bitcoin_rpc(&self) -> Result<Client, BridgeCliError> {
        match self.bitcoin_config {
            Some(ref config) => {
                let auth = Auth::UserPass(
                    config.user.expose_secret().into(),
                    config.password.expose_secret().into(),
                );
                let rpc = Client::new(config.url.as_str(), auth).await?;
                rpc.ping().await?;
                Ok(rpc)
            }
            None => Err(eyre::eyre!("Bitcoin RPC configuration not found in config").into()),
        }
    }

    /// Creates a default configuration based on the network.
    pub fn from_network(network: Network) -> Self {
        let mut config = BridgeCliConfig {
            network,
            ..BridgeCliConfig::default()
        };

        match network {
            Network::Regtest => {
                config.bitcoin_config = Some(BitcoinConfig {
                    url: Url::parse("http://localhost:18443/").expect("Valid url"),
                    password: SecretString::from("admin".to_string()),
                    user: SecretString::from("admin".to_string()),
                });
            }
            Network::Bitcoin => {
                config.citrea_chain_id = 1;
                config.citrea_backend_endpoint =
                    Url::parse("https://api.citrea.xyz/").expect("Valid url");
                config.citrea_rpc_url = Url::parse("https://rpc.citrea.xyz/").expect("Valid url");
                config.mempool_api_url =
                    Url::parse("https://mempool.space/api/").expect("Valid url");
                config.aggregated_public_key = XOnlyPublicKey::from_str(
                    "24280baf12b3532692fe42f41852b3122a509731c8f5462f88bc22391d7d7376",
                )
                .unwrap();
            }
            Network::Testnet4 => {
                config.citrea_chain_id = 1;
                config.citrea_backend_endpoint =
                    Url::parse("https://api.testnet.citrea.xyz/").expect("Valid url");
                config.citrea_rpc_url =
                    Url::parse("https://rpc.testnet.citrea.xyz/").expect("Valid url");
                config.mempool_api_url =
                    Url::parse("https://mempool.space/testnet4/api/").expect("Valid url");
                config.aggregated_public_key = XOnlyPublicKey::from_str(
                    "24280baf12b3532692fe42f41852b3122a509731c8f5462f88bc22391d7d7376",
                )
                .unwrap();
            }
            Network::Signet => {
                config.citrea_chain_id = 62298;
                config.citrea_backend_endpoint =
                    Url::parse("https://api.devnet.citrea.xyz/").expect("Valid url");
                config.citrea_rpc_url =
                    Url::parse("https://rpc.devnet.citrea.xyz/").expect("Valid url");
                config.mempool_api_url =
                    Url::parse("https://mempool.devnet.citrea.xyz/api/").expect("Valid url");
                config.aggregated_public_key = XOnlyPublicKey::from_str(
                    "24280baf12b3532692fe42f41852b3122a509731c8f5462f88bc22391d7d7376",
                )
                .unwrap();
            }
            _ => panic!("Network {network} is not supported!"), // This will only happen if [`Network`] has new fields
        };

        config
    }

    pub fn get_withdrawal_sign_url(&self) -> &'static str {
        match self.network {
            Network::Bitcoin => "https://citrea.xyz/withdrawal/sign",
            Network::Testnet | Network::Testnet4 => "https://citrea.xyz/withdrawal/sign", // #43
            Network::Signet => "https://devnet.citrea.xyz/withdrawal/sign",
            Network::Regtest => "http://127.0.0.1:12345",
        }
    }
}

impl Default for BridgeCliConfig {
    /// Defaults to regtest, which will only be used in tests.
    fn default() -> Self {
        Self {
            network: Network::Regtest,
            aggregated_public_key: XOnlyPublicKey::from_str(
                "24280baf12b3532692fe42f41852b3122a509731c8f5462f88bc22391d7d7376",
            )
            .unwrap(),
            mempool_api_url: Url::parse("https://127.0.0.1/").unwrap(),
            citrea_chain_id: 5655,
            citrea_backend_endpoint: Url::parse("https://127.0.0.1/").unwrap(),
            citrea_rpc_url: Url::parse("https://127.0.0.1/").unwrap(),
            user_takes_after: 200,
            bridge_amount: Amount::from_sat(1_000_000_000),
            bridge_contract_address: "0x3100000000000000000000000000000000000002".to_string(),
            bitcoin_config: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write};

    #[test]
    fn test_constants() {
        let config = BridgeCliConfig::default();

        assert_eq!(config.bridge_amount, Amount::from_sat(1_000_000_000));
        assert_eq!(config.user_takes_after, 200);
        assert_eq!(
            UNSPENDABLE_XONLY_PUBKEY.to_string(),
            "93c7378d96518a75448821c4f7c8f4bae7ce60f804d03d1f0628dd5dd0f5de51"
        );
    }

    #[test]
    fn parse_from_file() {
        let file_name = "parse_from_file";

        let invalid_content = "invalid file content";
        let mut file = File::create(file_name).unwrap();
        file.write_all(invalid_content.as_bytes()).unwrap();
        assert!(BridgeCliConfig::try_parse_file(file_name.into()).is_err());

        // Read first example test file use for this test.
        let base_path = env!("CARGO_MANIFEST_DIR");
        let config_path = format!("{}/bridge_cli_config.toml", base_path);
        let content = fs::read_to_string(config_path).unwrap();
        let mut file = File::create(file_name).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let read_config = BridgeCliConfig::try_parse_file(file_name.into()).unwrap();

        // Check some of the fields.
        assert_eq!(read_config.user_takes_after, 200);
        assert_eq!(read_config.network, Network::Regtest);
        assert_eq!(
            read_config.bitcoin_config.unwrap().url.as_str(),
            "http://127.0.0.1:18443/"
        );

        fs::remove_file(file_name).unwrap();
    }

    #[test]
    fn parse_from_file_with_invalid_headers() {
        let file_name = "parse_from_file_with_invalid_headers";
        let content = "[header1]
        num_verifiers = 4

        [header2]
        confirmation_threshold = 1
        network = \"regtest\"
        bitcoin_rpc_url = \"http://localhost:18443\"
        bitcoin_rpc_user = \"admin\"
        bitcoin_rpc_password = \"admin\"\n";
        let mut file = File::create(file_name).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        assert!(BridgeCliConfig::try_parse_file(file_name.into()).is_err());

        fs::remove_file(file_name).unwrap();
    }
}
