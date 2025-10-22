use bitcoin::Network;
use clap::builder::TypedValueParser;
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

impl std::fmt::Display for CliNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CliNetwork::Bitcoin => "bitcoin",
            CliNetwork::Testnet4 => "testnet4",
            CliNetwork::Signet => "signet",
            CliNetwork::Regtest => "regtest",
        };
        write!(f, "{}", s)
    }
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

/// Custom error message that shows aliases
fn network_error(invalid: &str) -> String {
    format!(
        "invalid value '{}' for '--network <NETWORK>'\n\
         \n\
         Possible values:\n\
         - bitcoin (alias: mainnet)\n\
         - testnet4 (alias: testnet)\n\
         - signet (alias: devnet)\n\
         - regtest\n\
         \n\
         For more information, try '--help'.",
        invalid
    )
}

/// Custom parser that shows aliases in error messages
#[derive(Clone)]
pub struct NetworkParser;

impl TypedValueParser for NetworkParser {
    type Value = CliNetwork;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let s = value.to_string_lossy();
        // Try to parse using ValueEnum (handles aliases automatically)
        CliNetwork::from_str(&s, true).map_err(|_| {
            clap::Error::raw(clap::error::ErrorKind::InvalidValue, network_error(&s)).with_cmd(cmd)
        })
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        CliNetwork::value_variants()
            .iter()
            .map(|v| v.to_possible_value())
            .collect::<Option<Vec<_>>>()
            .map(|v| Box::new(v.into_iter()) as _)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_names() {
        use clap::ValueEnum;
        assert_eq!(
            CliNetwork::from_str("bitcoin", true).unwrap(),
            CliNetwork::Bitcoin
        );
        assert_eq!(
            CliNetwork::from_str("testnet4", true).unwrap(),
            CliNetwork::Testnet4
        );
        assert_eq!(
            CliNetwork::from_str("signet", true).unwrap(),
            CliNetwork::Signet
        );
        assert_eq!(
            CliNetwork::from_str("regtest", true).unwrap(),
            CliNetwork::Regtest
        );
    }

    #[test]
    fn test_aliases() {
        use clap::ValueEnum;
        assert_eq!(
            CliNetwork::from_str("mainnet", true).unwrap(),
            CliNetwork::Bitcoin
        );
        assert_eq!(
            CliNetwork::from_str("testnet", true).unwrap(),
            CliNetwork::Testnet4
        );
        assert_eq!(
            CliNetwork::from_str("devnet", true).unwrap(),
            CliNetwork::Signet
        );
    }

    #[test]
    fn test_case_insensitive() {
        use clap::ValueEnum;
        assert_eq!(
            CliNetwork::from_str("BITCOIN", true).unwrap(),
            CliNetwork::Bitcoin
        );
        assert_eq!(
            CliNetwork::from_str("Bitcoin", true).unwrap(),
            CliNetwork::Bitcoin
        );
        assert_eq!(
            CliNetwork::from_str("MainNet", true).unwrap(),
            CliNetwork::Bitcoin
        );
        assert_eq!(
            CliNetwork::from_str("MAINNET", true).unwrap(),
            CliNetwork::Bitcoin
        );
        assert_eq!(
            CliNetwork::from_str("DevNet", true).unwrap(),
            CliNetwork::Signet
        );
    }

    #[test]
    fn test_invalid_network() {
        use clap::ValueEnum;
        assert!(CliNetwork::from_str("invalid", true).is_err());
        assert!(CliNetwork::from_str("ethereum", true).is_err());
    }

    #[test]
    fn test_conversion_to_bitcoin_network() {
        assert_eq!(Network::from(CliNetwork::Bitcoin), Network::Bitcoin);
        assert_eq!(Network::from(CliNetwork::Testnet4), Network::Testnet4);
        assert_eq!(Network::from(CliNetwork::Signet), Network::Signet);
        assert_eq!(Network::from(CliNetwork::Regtest), Network::Regtest);
    }
}
