use bitcoin::Network;
use clap::ValueEnum;

/// Local copy of [`bitcoin::Network`] with user-friendly aliases
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum CliNetwork {
    #[value(alias = "mainnet")]
    Bitcoin,
    #[value(alias = "testnet")]
    Testnet4,
    #[value(alias = "devnet")]
    Signet,
    Regtest,
}

impl From<CliNetwork> for Network {
    fn from(value: CliNetwork) -> Self {
        match value {
            CliNetwork::Bitcoin => Network::Bitcoin,
            CliNetwork::Testnet4 => Network::Testnet4,
            CliNetwork::Signet => Network::Signet,
            CliNetwork::Regtest => Network::Regtest,
        }
    }
}
