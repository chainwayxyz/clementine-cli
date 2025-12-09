//! # Configuration Options
//!
//! Configuration options provided here are used to make a request to Clementine.

use crate::{errors::BridgeCliError, get_clementine_config_path_with_existence_check};
use bitcoin::{Amount, Network, XOnlyPublicKey};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use eyre::{Context, Result};
use reqwest::Url;
use secrecy::{CloneableSecret, ExposeSecret, SecretBox};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    str::FromStr,
    sync::LazyLock,
};
use thiserror::Error;
use zeroize::Zeroize;

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

    #[error(transparent)]
    Other(#[from] eyre::Report),
}

#[derive(Serialize, Deserialize, Zeroize, Clone)]
#[zeroize(drop)]
pub struct ApiSecret(pub String);

impl secrecy::SerializableSecret for ApiSecret {}

impl CloneableSecret for ApiSecret {}

impl From<String> for ApiSecret {
    fn from(value: String) -> Self {
        ApiSecret(value)
    }
}

pub trait ToSecretBox {
    fn to_secret_box(self) -> secrecy::SecretBox<ApiSecret>;
}

impl ToSecretBox for &str {
    fn to_secret_box(self) -> secrecy::SecretBox<ApiSecret> {
        secrecy::SecretBox::from(Box::new(ApiSecret(self.to_owned())))
    }
}
impl ToSecretBox for String {
    fn to_secret_box(self) -> secrecy::SecretBox<ApiSecret> {
        secrecy::SecretBox::from(Box::new(ApiSecret(self)))
    }
}

/// [`BridgeCliConfig`]s for each network.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NetworkConfigs {
    pub bitcoin: BridgeCliConfig,
    pub testnet4: BridgeCliConfig,
    pub signet: BridgeCliConfig,
    pub regtest: BridgeCliConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BridgeCliConfig {
    pub network: Network,
    pub aggregated_public_key: XOnlyPublicKey,
    pub mempool_api_url: Option<Url>,
    pub citrea_chain_id: u64,
    pub citrea_rpc_url: Url,
    pub citrea_backend_endpoint: Url,
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
                    "24280baf12b3532692fe42f41852b3122a509731c8f5462f88bc22391d7d7376",
                )
                .unwrap(),
                mempool_api_url: Some(Url::parse("https://mempool.space/api/").unwrap()),
                citrea_chain_id: 0,
                citrea_rpc_url: Url::parse("https://rpc.citrea.xyz/").unwrap(),
                citrea_backend_endpoint: Url::parse("https://api.citrea.xyz/").unwrap(),
                user_takes_after: 200,
                bridge_amount: Amount::from_sat(1_000_000_000),
                optimistic_withdrawal_amount: Amount::from_sat(999_999_760),
                operator_withdrawal_amount: Amount::from_sat(997_000_000),
                dust_utxo_amount: Amount::from_sat(330),
                bridge_contract_address: "0x3100000000000000000000000000000000000002".into(),
                move_tx_finalization_blocks: 6,
                bitcoin_config: Some(BitcoinConfig {
                    url: Url::parse("http://127.0.0.1:18443/").unwrap(),
                    user: "admin".to_secret_box(),
                    password: "admin".to_secret_box(),
                }),
            },

            Network::Testnet4 => Self {
                network,
                aggregated_public_key: XOnlyPublicKey::from_str(
                    "1e0f48f81dfa14d114f5d942f1e2d50771a4b019fa942606bb1f258b4b326bf7",
                )
                .unwrap(),
                mempool_api_url: Some(Url::parse("https://mempool.space/testnet4/api/").unwrap()),
                citrea_chain_id: 5115,
                citrea_rpc_url: Url::parse("https://rpc.testnet.citrea.xyz/").unwrap(),
                citrea_backend_endpoint: Url::parse("https://api.testnet.citrea.xyz/").unwrap(),
                user_takes_after: 200,
                bridge_amount: Amount::from_sat(1_000_000_000),
                optimistic_withdrawal_amount: Amount::from_sat(999_999_760),
                operator_withdrawal_amount: Amount::from_sat(997_000_000),
                dust_utxo_amount: Amount::from_sat(330),
                bridge_contract_address: "0x3100000000000000000000000000000000000002".into(),
                move_tx_finalization_blocks: 100,
                bitcoin_config: Some(BitcoinConfig {
                    url: Url::parse("http://127.0.0.1:18443/").unwrap(),
                    user: "admin".to_secret_box(),
                    password: "admin".to_secret_box(),
                }),
            },

            Network::Signet => Self {
                network,
                aggregated_public_key: XOnlyPublicKey::from_str(
                    "359fa25e72d66cacd545a9d43f9757b8fed3f04fda0c665551fe093139e819dc",
                )
                .unwrap(),
                mempool_api_url: Some(
                    Url::parse("https://mempool.devnet.citrea.xyz/api/").unwrap(),
                ),
                citrea_chain_id: 62298,
                citrea_rpc_url: Url::parse("https://rpc.devnet.citrea.xyz/").unwrap(),
                citrea_backend_endpoint: Url::parse("https://api.devnet.citrea.xyz/").unwrap(),
                user_takes_after: 200,
                bridge_amount: Amount::from_sat(1_000_000_000),
                optimistic_withdrawal_amount: Amount::from_sat(999_999_760),
                operator_withdrawal_amount: Amount::from_sat(997_000_000),
                dust_utxo_amount: Amount::from_sat(330),
                bridge_contract_address: "0x3100000000000000000000000000000000000002".into(),
                move_tx_finalization_blocks: 5,
                bitcoin_config: Some(BitcoinConfig {
                    url: Url::parse("http://127.0.0.1:38332/").unwrap(),
                    user: "admin".to_secret_box(),
                    password: "admin".to_secret_box(),
                }),
            },

            Network::Regtest => Self {
                network,
                aggregated_public_key: XOnlyPublicKey::from_str(
                    "30ff95ec2726938072a2009f3276cd8fba2363d9284a7eb01217b2f302eb8577",
                )
                .unwrap(),
                mempool_api_url: Some(Url::parse("https://127.0.0.1/").unwrap()),
                citrea_chain_id: 5655,
                citrea_rpc_url: Url::parse("https://127.0.0.1:12345/").unwrap(),
                citrea_backend_endpoint: Url::parse("https://127.0.0.1/").unwrap(),
                user_takes_after: 200,
                bridge_amount: Amount::from_sat(1_000_000_000),
                optimistic_withdrawal_amount: Amount::from_sat(999_999_760),
                operator_withdrawal_amount: Amount::from_sat(997_000_000),
                dust_utxo_amount: Amount::from_sat(330),
                bridge_contract_address: "0x3100000000000000000000000000000000000002".into(),
                move_tx_finalization_blocks: 5,
                bitcoin_config: Some(BitcoinConfig {
                    url: Url::parse("http://127.0.0.1:20443/wallet/admin").unwrap(),
                    user: "admin".to_secret_box(),
                    password: "admin".to_secret_box(),
                }),
            },

            _ => panic!("Unsupported network in defaults: {network:?}"),
        }
    }
}

pub fn default_networks() -> NetworkConfigs {
    NetworkConfigs {
        bitcoin: BridgeCliConfig::defaults_for(Network::Bitcoin),
        testnet4: BridgeCliConfig::defaults_for(Network::Testnet4),
        signet: BridgeCliConfig::defaults_for(Network::Signet),
        regtest: BridgeCliConfig::defaults_for(Network::Regtest),
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
    pub password: SecretBox<ApiSecret>,
    pub user: SecretBox<ApiSecret>,
}

impl BridgeCliConfig {
    pub fn new() -> Self {
        BridgeCliConfig::default()
    }

    /// Tries to parse config file from home directory.
    pub fn try_parse_config(network: Network) -> Result<Self, ConfigErrors> {
        let config_path = get_clementine_config_path_with_existence_check().map_err(|_| {
            ConfigErrors::FileReadFailure(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Config file not found in home directory. Please run 'clementine-cli init' to create one.",
            ))
        })?;
        let config = Self::try_parse_file(config_path.clone(), network);
        if let Ok(config) = config {
            tracing::debug!("Using home configuration file: {config_path:?}");
            return Ok(config);
        }

        tracing::error!(
            "Configuration file is not parsable at path: {config_path:?}, Error: {:?}",
            config.err()
        );

        Err(ConfigErrors::FileReadFailure(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "Configuration file is not parsable at path: {:?}",
                config_path
            ),
        )))
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
            rest => return Err(ConfigErrors::UnsupportedNetwork(rest)),
        };

        // All of the URLs needs a trailing slash. If not present, add it.
        if config.mempool_api_url.is_some()
            && !config
                .mempool_api_url
                .as_ref()
                .expect("Checked in the first condition")
                .to_string()
                .ends_with("/")
        {
            let str_url = config.mempool_api_url.expect("Checked above").to_string() + "/";
            config.mempool_api_url =
                Some(Url::from_str(&str_url).wrap_err("Can't add trailing slash to URL")?);
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
                    config.user.expose_secret().0.clone(),
                    config.password.expose_secret().0.clone(),
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
                    password: "admin".to_secret_box(),
                    user: "admin".to_secret_box(),
                });
            }
            Network::Bitcoin => {
                config.citrea_chain_id = 1;
                config.citrea_backend_endpoint =
                    Url::parse("https://api.citrea.xyz/").expect("Valid url");
                config.citrea_rpc_url = Url::parse("https://rpc.citrea.xyz/").expect("Valid url");
                config.mempool_api_url =
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
                    Url::parse("https://rpc.testnet.citrea.xyz/").expect("Valid url");
                config.mempool_api_url =
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
                    Url::parse("https://rpc.devnet.citrea.xyz/").expect("Valid url");
                config.mempool_api_url =
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
            mempool_api_url: Some(Url::parse("https://127.0.0.1/").unwrap()),
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
