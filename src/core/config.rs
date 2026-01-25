//! # Configuration Options
//!
//! Configuration options provided here are used to make a request to Clementine.

use crate::{core::errors::BridgeCliError, get_clementine_config_path_with_existence_check};
use bitcoin::{Amount, Network, XOnlyPublicKey};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use eyre::{Context, Result};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    str::FromStr,
    sync::LazyLock,
};
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
    UnsupportedNetwork(Network),
    #[error(
        "Invalid configuration: You must configure exactly one of 'esplora_rest_api' or 'bitcoin_config', not both or neither"
    )]
    InvalidApiConfiguration,

    #[error(transparent)]
    Other(#[from] eyre::Report),
}

/// [`BridgeCliConfig`]s for each network.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NetworkConfigs {
    pub bitcoin: BridgeCliConfig,
    pub testnet4: BridgeCliConfig,
    pub signet: Option<BridgeCliConfig>,
    pub regtest: Option<BridgeCliConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BridgeCliConfig {
    pub network: Network,
    pub aggregated_public_key: XOnlyPublicKey,
    pub esplora_rest_api: Option<Url>,
    pub citrea_chain_id: u64,
    pub citrea_rpc_url: Option<Url>,
    pub citrea_backend_endpoint: Url,
    pub withdrawal_sign_url: Url,
    pub user_takes_after: u64,
    pub bridge_amount: Amount,
    pub optimistic_withdrawal_amount: Amount,
    pub operator_withdrawal_amount: Amount,
    pub dust_utxo_amount: Amount,
    pub bridge_contract_address: String,
    pub move_tx_finalization_blocks: u64,
    pub bitcoin_config: Option<BitcoinConfig>,
}

impl BridgeCliConfig {
    pub fn defaults_for(network: Network) -> Self {
        match network {
            Network::Bitcoin => Self {
                network,
                aggregated_public_key: XOnlyPublicKey::from_str(
                    "70458db6f75b129ad93878e7781eeb9ba24bf4bbd498d6a9d3f27827b25c8f81",
                )
                .unwrap(),
                esplora_rest_api: Some(Url::parse("https://mempool.space/api/").unwrap()),
                citrea_chain_id: 4114,
                citrea_rpc_url: None,
                citrea_backend_endpoint: Url::parse("https://api.mainnet.citrea.xyz/").unwrap(),
                withdrawal_sign_url: Url::parse("https://citrea.xyz/withdrawal/sign").unwrap(),
                user_takes_after: 200,
                bridge_amount: Amount::from_sat(1_000_000_000),
                optimistic_withdrawal_amount: Amount::from_sat(999_999_760),
                operator_withdrawal_amount: Amount::from_sat(997_000_000),
                dust_utxo_amount: Amount::from_sat(330),
                bridge_contract_address: "0x3100000000000000000000000000000000000002".into(),
                move_tx_finalization_blocks: 6,
                bitcoin_config: None,
            },

            Network::Testnet4 => Self {
                network,
                aggregated_public_key: XOnlyPublicKey::from_str(
                    "1e0f48f81dfa14d114f5d942f1e2d50771a4b019fa942606bb1f258b4b326bf7",
                )
                .unwrap(),
                esplora_rest_api: Some(Url::parse("https://mempool.space/testnet4/api/").unwrap()),
                citrea_chain_id: 5115,
                citrea_rpc_url: None,
                citrea_backend_endpoint: Url::parse("https://api.testnet.citrea.xyz/").unwrap(),
                withdrawal_sign_url: Url::parse("https://testnet.citrea.xyz/withdrawal/sign").unwrap(),
                user_takes_after: 200,
                bridge_amount: Amount::from_sat(1_000_000_000),
                optimistic_withdrawal_amount: Amount::from_sat(999_999_760),
                operator_withdrawal_amount: Amount::from_sat(997_000_000),
                dust_utxo_amount: Amount::from_sat(330),
                bridge_contract_address: "0x3100000000000000000000000000000000000002".into(),
                move_tx_finalization_blocks: 100,
                bitcoin_config: None,
            },

            Network::Signet => Self {
                network,
                aggregated_public_key: XOnlyPublicKey::from_str(
                    "359fa25e72d66cacd545a9d43f9757b8fed3f04fda0c665551fe093139e819dc",
                )
                .unwrap(),
                esplora_rest_api: Some(
                    Url::parse("https://mempool.devnet.citrea.xyz/api/").unwrap(),
                ),
                citrea_chain_id: 62298,
                citrea_rpc_url: None,
                citrea_backend_endpoint: Url::parse("https://api.devnet.citrea.xyz/").unwrap(),
                withdrawal_sign_url: Url::parse("https://devnet.citrea.xyz/withdrawal/sign").unwrap(),
                user_takes_after: 200,
                bridge_amount: Amount::from_sat(1_000_000_000),
                optimistic_withdrawal_amount: Amount::from_sat(999_999_760),
                operator_withdrawal_amount: Amount::from_sat(997_000_000),
                dust_utxo_amount: Amount::from_sat(330),
                bridge_contract_address: "0x3100000000000000000000000000000000000002".into(),
                move_tx_finalization_blocks: 5,
                bitcoin_config: None,
            },

            Network::Regtest => Self {
                network,
                aggregated_public_key: XOnlyPublicKey::from_str(
                    "30ff95ec2726938072a2009f3276cd8fba2363d9284a7eb01217b2f302eb8577",
                )
                .unwrap(),
                esplora_rest_api: Some(Url::parse("https://127.0.0.1/").unwrap()),
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
                bitcoin_config: None,
            },

            _ => panic!("Unsupported network in defaults: {network:?}"),
        }
    }
}

pub fn default_networks() -> NetworkConfigs {
    NetworkConfigs {
        bitcoin: BridgeCliConfig::defaults_for(Network::Bitcoin),
        testnet4: BridgeCliConfig::defaults_for(Network::Testnet4),
        signet: None,
        regtest: None,
    }
}

pub fn write_config_to(path: &Path, cfgs: &NetworkConfigs) -> Result<(), BridgeCliError> {
    let toml_txt = toml::to_string_pretty(cfgs).map_err(|e| {
        tracing::error!("Failed to serialize config to TOML: {:?}", e);
        BridgeCliError::Eyre(eyre::eyre!("Failed to serialize config to TOML"))
    })?;

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }

    fs::write(path, toml_txt)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BitcoinConfig {
    pub url: Url,
    pub user: String,
    pub password: String,
}

impl BridgeCliConfig {
    pub fn new() -> Self {
        BridgeCliConfig::default()
    }

    /// Tries to parse config file from home directory.
    pub fn try_parse_config(network: Network) -> Result<Self, ConfigErrors> {
        tracing::debug!("Trying to read and parse configuration file from home directory...");
        let config_path = get_clementine_config_path_with_existence_check().map_err(|e| {
            tracing::error!("Failed to locate config file: {}", e);
            ConfigErrors::FileReadFailure(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Config file not found in home directory. Please run 'clementine-cli init' to create one.",
            ))
        })?;
        let config = Self::try_parse_file(config_path.clone(), network);
        match config {
            Ok(cfg) => {
                tracing::debug!("Using home configuration file: {config_path:?}");
                Ok(cfg)
            }
            Err(ConfigErrors::UnsupportedNetwork(_)) => {
                tracing::error!(
                    "Configuration file does not support the selected network at path: {config_path:?}"
                );
                Err(ConfigErrors::UnsupportedNetwork(network))
            }
            Err(e) => {
                tracing::error!(
                    "Configuration file is not parsable at path {config_path:?}: {}",
                    e
                );
                Err(ConfigErrors::FileReadFailure(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "Configuration file is not parsable at path: {:?}",
                        config_path
                    ),
                )))
            }
        }
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
            Network::Signet => network_configs
                .signet
                .ok_or(ConfigErrors::UnsupportedNetwork(network))?,
            Network::Regtest => network_configs
                .regtest
                .ok_or(ConfigErrors::UnsupportedNetwork(network))?,
            rest => return Err(ConfigErrors::UnsupportedNetwork(rest)),
        };

        // Validate that exactly one API is configured
        match (&config.esplora_rest_api, &config.bitcoin_config) {
            (None, None) => return Err(ConfigErrors::InvalidApiConfiguration),
            (Some(_), Some(_)) => return Err(ConfigErrors::InvalidApiConfiguration),
            _ => {} // Exactly one is configured, which is valid
        }

        // All of the URLs needs a trailing slash. If not present, add it.
        if config.esplora_rest_api.is_some()
            && !config
                .esplora_rest_api
                .as_ref()
                .expect("Checked in the first condition")
                .to_string()
                .ends_with("/")
        {
            let str_url = config.esplora_rest_api.expect("Checked above").to_string() + "/";
            config.esplora_rest_api =
                Some(Url::from_str(&str_url).wrap_err("Can't add trailing slash to URL")?);
        }
        if !config.citrea_backend_endpoint.to_string().ends_with("/") {
            let str_url = config.citrea_backend_endpoint.to_string() + "/";
            config.citrea_backend_endpoint =
                Url::from_str(&str_url).wrap_err("Can't add trailing slash to URL")?;
        }

        if config
            .citrea_rpc_url
            .as_ref()
            .is_some_and(|url| !url.to_string().ends_with("/"))
        {
            let str_url = config
                .citrea_rpc_url
                .as_ref()
                .expect("Cannot fail, checked above")
                .to_string()
                + "/";
            config.citrea_rpc_url =
                Some(Url::from_str(&str_url).wrap_err("Can't add trailing slash to URL")?);
        }

        Ok(config)
    }

    pub async fn connect_to_bitcoin_rpc(&self) -> Result<Client, BridgeCliError> {
        match self.bitcoin_config {
            Some(ref config) => {
                let auth = Auth::UserPass(config.user.clone(), config.password.clone());
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
                // No bitcoin_config needed - using Bitcoin Esplora API only
            }
            Network::Bitcoin => {
                config.citrea_chain_id = 1;
                config.citrea_backend_endpoint =
                    Url::parse("https://api.citrea.xyz/").expect("Valid url");
                config.citrea_rpc_url =
                    Some(Url::parse("https://rpc.citrea.xyz/").expect("Valid url"));
                config.esplora_rest_api =
                    Some(Url::parse("https://mempool.space/api/").expect("Valid url"));
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
                    Some(Url::parse("https://rpc.testnet.citrea.xyz/").expect("Valid url"));
                config.esplora_rest_api =
                    Some(Url::parse("https://mempool.space/testnet4/api/").expect("Valid url"));
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
                    Some(Url::parse("https://rpc.devnet.citrea.xyz/").expect("Valid url"));
                config.esplora_rest_api =
                    Some(Url::parse("https://mempool.devnet.citrea.xyz/api/").expect("Valid url"));
                config.aggregated_public_key = XOnlyPublicKey::from_str(
                    "24280baf12b3532692fe42f41852b3122a509731c8f5462f88bc22391d7d7376",
                )
                .unwrap();
            }
            _ => panic!("Network {network} is not supported!"), // This will only happen if [`Network`] has new fields
        };

        config
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
            esplora_rest_api: Some(Url::parse("https://127.0.0.1/").unwrap()),
            citrea_chain_id: 5655,
            citrea_backend_endpoint: Url::parse("https://127.0.0.1/").unwrap(),
            citrea_rpc_url: Some(Url::parse("https://127.0.0.1/").unwrap()),
            withdrawal_sign_url: Url::parse("http://127.0.0.1:12345/").unwrap(),
            user_takes_after: 200,
            bridge_amount: Amount::from_sat(1_000_000_000),
            optimistic_withdrawal_amount: Amount::from_sat(1_000_000_000),
            operator_withdrawal_amount: Amount::from_sat(997000000),
            dust_utxo_amount: Amount::from_sat(330),
            bridge_contract_address: "0x3100000000000000000000000000000000000002".to_string(),
            bitcoin_config: None,
            move_tx_finalization_blocks: 5,
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
    fn test_bridge_amount_conversion_to_btc() {
        let config = BridgeCliConfig::default();
        assert_eq!(config.bridge_amount.to_btc(), 10.0);
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
