use std::str::FromStr;

use bitcoin::Network;
use clap::builder::{PossibleValue, TypedValueParser};
use clap::error::{Error, ErrorKind};
use colored::Colorize;

/// Local copy of [`bitcoin::Network`]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CliNetwork {
    Bitcoin,
    Testnet4,
    Signet,
    Regtest,
}

/// If the CliNetwork enum or aliases are changed, update `NETS` below and this help message.
pub const NETWORK_HELP_MESSAGE: &str =
    "Bitcoin network to use. [aliases: bitcoin=mainnet, testnet4=testnet, signet=devnet]";

/// One source of truth for canonical names, aliases and pretty labels.
struct NetRow {
    canon: &'static str,
    aliases: &'static [&'static str],
    variant: CliNetwork,
    pretty: &'static str, // right-hand label for the error list, e.g. "→ Bitcoin"
}

const NETS: &[NetRow] = &[
    NetRow {
        canon: "bitcoin",
        aliases: &["mainnet"],
        variant: CliNetwork::Bitcoin,
        pretty: "Bitcoin",
    },
    NetRow {
        canon: "testnet4",
        aliases: &["testnet"],
        variant: CliNetwork::Testnet4,
        pretty: "Testnet4",
    },
    NetRow {
        canon: "signet",
        aliases: &["devnet"],
        variant: CliNetwork::Signet,
        pretty: "Signet",
    },
    NetRow {
        canon: "regtest",
        aliases: &[],
        variant: CliNetwork::Regtest,
        pretty: "Regtest",
    },
];

fn find_network(s: &str) -> Option<CliNetwork> {
    let s = s.to_lowercase();
    for row in NETS {
        if row.canon == s || row.aliases.iter().any(|a| *a == s) {
            return Some(row.variant);
        }
    }
    None
}

fn canon_of(n: CliNetwork) -> &'static str {
    NETS.iter().find(|r| r.variant == n).unwrap().canon
}

fn build_network_error(bad: &str) -> String {
    // compute column widths
    let mut max_canon = 0usize;
    let mut max_alias = 0usize;
    for row in NETS {
        max_canon = max_canon.max(row.canon.len());
        max_alias = max_alias.max(row.aliases.join(" | ").len());
    }

    let mut out = String::new();
    out.push_str(&format!(
        "invalid value '{}' for '{}':\n{}\n",
        bad.bold().yellow(),
        "--network <NETWORK>".bold(),
        "Allowed values:".bold().yellow(),
    ));

    for row in NETS {
        let alias_str = row.aliases.join(" | ");
        // Reserve alias column even if empty so the arrow aligns.
        let lhs = if alias_str.is_empty() {
            // 3 spaces to occupy where " | " would be, plus alias padding
            format!(
                "- {:<wc$}   {:<wa$}",
                row.canon,
                "",
                wc = max_canon,
                wa = max_alias
            )
        } else {
            format!(
                "- {:<wc$} | {:<wa$}",
                row.canon,
                alias_str,
                wc = max_canon,
                wa = max_alias
            )
        };

        out.push_str(&format!("  {}  → {}\n", lhs.green(), row.pretty.green()));
    }

    out.push_str(&format!(
        "\nFor more information, try '{}'.\n",
        "--help".bold()
    ));
    out
}

impl FromStr for CliNetwork {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        find_network(s).ok_or_else(|| build_network_error(s))
    }
}

impl std::fmt::Display for CliNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", canon_of(*self))
    }
}

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
        let val = value.to_string_lossy();
        CliNetwork::from_str(&val)
            // attach the current Command so clap applies its color policy
            .map_err(|msg| Error::raw(ErrorKind::InvalidValue, msg).with_cmd(cmd))
    }

    // Powers `[possible values: ...]` in `--help`, with aliases discoverable.
    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(NETS.iter().map(|row| {
            let mut pv = PossibleValue::new(row.canon);
            for &a in row.aliases {
                pv = pv.alias(a);
            }
            pv
        })))
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
