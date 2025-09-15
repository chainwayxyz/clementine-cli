//! # Configuration Options
//!
//! Configuration options provided here are used to make a request to Clementine.

use crate::{errors::BridgeCliError, get_clementine_home_dir};
use bitcoin::{Amount, Network, XOnlyPublicKey};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use eyre::{Context, Result};
use reqwest::Url;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::{fs::File, io::Read, path::PathBuf, str::FromStr, sync::LazyLock};
use thiserror::Error;

pub static UNSPENDABLE_XONLY_PUBKEY: LazyLock<XOnlyPublicKey> = LazyLock::new(|| {
    XOnlyPublicKey::from_str("50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0")
        .unwrap()
});

#[derive(Debug, Error)]
pub enum ConfigErrors {
    #[error("Can't read configuration file: {0}")]
    FileReadFailure(#[from] std::io::Error),
    #[error("Can't parse TOML file: {0}")]
    TomlError(#[from] toml::de::Error),
    #[error("Network {0} is not supported!")]
    UnsportedNetwork(Network),

    #[error(transparent)]
    Other(#[from] eyre::Report),
}

/// [`BridgeCliConfig`]s for each network.
#[derive(Debug, Clone, Deserialize)]
pub struct NetworkConfigs {
    pub bitcoin: BridgeCliConfig,
    pub testnet4: BridgeCliConfig,
    pub signet: BridgeCliConfig,
    pub regtest: BridgeCliConfig,
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
    pub optimistic_withdrawal_amount: Amount,
    pub operator_withdrawal_amount: Amount,
    pub dust_utxo_amount: Amount,
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

    /// Tries to parse config file with order:
    ///
    /// 1. If given, custom config path
    /// 2. `~/.clementine/bridge_cli_config.toml`
    /// 3. `$PWD/bridge_cli_config.toml`
    pub fn try_parse_config(path: Option<PathBuf>, network: Network) -> Result<Self, ConfigErrors> {
        if let Some(path) = path {
            tracing::debug!("Using given configuration file: {path:?}");
            return Self::try_parse_file(path, network);
        }

        let home_dir = get_clementine_home_dir().wrap_err("Can't get Clementine home directory")?;
        let config_dir = home_dir.join("bridge_cli_config.toml");
        if let Ok(config) = Self::try_parse_file(config_dir.clone(), network) {
            tracing::debug!("Using home configuration file: {config_dir:?}");
            return Ok(config);
        }

        let mut current_dir = std::env::current_dir().unwrap();
        current_dir.push("bridge_cli_config.toml");
        tracing::debug!("Using configuration file at the current directory: {current_dir:?}");
        Self::try_parse_file(current_dir, network)
    }

    /// Read contents of a TOML file and generate a [`CliConfig`].
    fn try_parse_file(path: PathBuf, network: Network) -> Result<Self, ConfigErrors> {
        let mut contents = String::new();

        let mut file = File::open(&path)?;
        file.read_to_string(&mut contents)?;

        let network_configs = toml::from_str::<NetworkConfigs>(&contents)?;

        let mut config = match network {
            Network::Bitcoin => network_configs.bitcoin,
            Network::Testnet4 => network_configs.testnet4,
            Network::Signet => network_configs.signet,
            Network::Regtest => network_configs.regtest,
            rest => return Err(ConfigErrors::UnsportedNetwork(rest)),
        };

        // All of the URLs needs a trailing slash. If not present, add it.
        if !config.mempool_api_url.to_string().ends_with("/") {
            let str_url = config.mempool_api_url.to_string() + "/";
            config.mempool_api_url =
                Url::from_str(&str_url).wrap_err("Can't add trailing slash to URL")?;
        }
        if !config.citrea_backend_endpoint.to_string().ends_with("/") {
            let str_url = config.citrea_backend_endpoint.to_string() + "/";
            config.citrea_backend_endpoint =
                Url::from_str(&str_url).wrap_err("Can't add trailing slash to URL")?;
        }
        if !config.citrea_rpc_url.to_string().ends_with("/") {
            let str_url = config.citrea_rpc_url.to_string() + "/";
            config.citrea_rpc_url =
                Url::from_str(&str_url).wrap_err("Can't add trailing slash to URL")?;
        }

        Ok(config)
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

    #[cfg(test)]
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
                config.citrea_chain_id = 5115;
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
            Network::Testnet4 => "https://citrea.xyz/withdrawal/sign", // #43
            Network::Signet => "https://devnet.citrea.xyz/withdrawal/sign",
            Network::Regtest => "http://127.0.0.1:12345",
            rest => panic!("Network {rest} is not supported!"),
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
            optimistic_withdrawal_amount: Amount::from_sat(1_000_000_000),
            operator_withdrawal_amount: Amount::from_sat(997000000),
            dust_utxo_amount: Amount::from_sat(330),
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
    }

    #[test]
    fn parse_from_file() {
        let file_name = "parse_from_file";

        let invalid_content = "invalid file content";
        let mut file = File::create(file_name).unwrap();
        file.write_all(invalid_content.as_bytes()).unwrap();
        assert!(BridgeCliConfig::try_parse_file(file_name.into(), Network::Testnet4).is_err());

        // Read first example test file use for this test.
        let base_path = env!("CARGO_MANIFEST_DIR");
        let config_path = format!("{}/bridge_cli_config.toml", base_path);
        let content = fs::read_to_string(config_path).unwrap();
        let mut file = File::create(file_name).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let read_config =
            BridgeCliConfig::try_parse_file(file_name.into(), Network::Testnet4).unwrap();

        // Check some of the fields.
        assert_eq!(read_config.user_takes_after, 200);
        assert_eq!(read_config.network, Network::Testnet4);
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

        assert!(BridgeCliConfig::try_parse_file(file_name.into(), Network::Regtest).is_err());

        fs::remove_file(file_name).unwrap();
    }
}
