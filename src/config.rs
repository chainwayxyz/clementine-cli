// Configuration for Clementine CLI

use std::{str::FromStr, sync::LazyLock};

use bitcoin::{
    Amount, Network, XOnlyPublicKey,
    secp256k1::{Parity, PublicKey},
};

pub static UNSPENDABLE_XONLY_PUBKEY: LazyLock<XOnlyPublicKey> = LazyLock::new(|| {
    XOnlyPublicKey::from_str("93c7378d96518a75448821c4f7c8f4bae7ce60f804d03d1f0628dd5dd0f5de51")
        .unwrap()
});

pub const BRIDGE_AMOUNT: Amount = Amount::from_sat(1_000_000_000);

pub const USER_TAKES_AFTER: u64 = 200;

/// Get backend endpoint for a specific network
pub fn get_backend_endpoint(network: Network) -> &'static str {
    match network {
        Network::Bitcoin => "https://api.citrea.xyz/",
        Network::Testnet4 | Network::Testnet => "https://api.testnet.citrea.xyz/",
        Network::Signet => "https://api.devnet.citrea.xyz/",
        _ => {
            panic!("No backend endpoint configured for network {:?}", network);
        }
    }
}

/// Get verifier public keys for a specific network
pub fn get_verifier_pks(network: Network) -> Vec<PublicKey> {
    match network {
        Network::Bitcoin => unimplemented!(),
        Network::Testnet4 => vec![
            XOnlyPublicKey::from_str(
                "24280baf12b3532692fe42f41852b3122a509731c8f5462f88bc22391d7d7376",
            )
            .unwrap()
            .public_key(Parity::Odd),
        ],
        Network::Signet => vec![
            PublicKey::from_str(
                "034f355bdcb7cc0af728ef3cceb9615d90684bb5b2ca5f859ab0f0b704075871aa",
            )
            .unwrap(),
            PublicKey::from_str(
                "02466d7fcae563e5cb09a0d1870bb580344804617879a14949cf22285f1bae3f27",
            )
            .unwrap(),
            PublicKey::from_str(
                "023c72addb4fdf09af94f0c94d7fe92a386a7e70cf8a1d85916386bb2535c7b1b1",
            )
            .unwrap(),
            PublicKey::from_str(
                "032c0b7cf95324a07d05398b240174dc0c2be444d96b159aa6c7f7b1e668680991",
            )
            .unwrap(),
        ],
        _ => vec![],
    }
}

pub fn get_mempool_api_url(network: Network) -> &'static str {
    match network {
        Network::Bitcoin => "https://mempool.space/api/",
        Network::Testnet4 => "https://mempool.space/testnet4/api/",
        Network::Signet => "https://mempool.devnet.citrea.xyz/api/",
        _ => unimplemented!(),
    }
}

pub fn get_chain_id(network: Network) -> u64 {
    match network {
        // Network::Bitcoin => 1,
        // Network::Testnet4 => 4,
        Network::Signet => 62298,
        Network::Regtest => 5655,
        _ => unimplemented!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_backend_endpoint() {
        assert_eq!(
            get_backend_endpoint(Network::Bitcoin),
            "https://api.citrea.xyz/"
        );
        assert_eq!(
            get_backend_endpoint(Network::Testnet4),
            "https://api.testnet.citrea.xyz/"
        );
        assert_eq!(
            get_backend_endpoint(Network::Signet),
            "https://api.devnet.citrea.xyz/"
        );
        assert_eq!(
            get_backend_endpoint(Network::Testnet),
            "https://api.testnet.citrea.xyz/"
        );
    }

    #[test]
    #[should_panic]
    fn test_get_backend_endpoint_regtest() {
        get_backend_endpoint(Network::Regtest);
    }

    #[test]
    fn test_get_verifier_pks_other() {
        let pks = get_verifier_pks(Network::Regtest);
        assert!(pks.is_empty());
    }

    #[test]
    fn test_constants() {
        assert_eq!(BRIDGE_AMOUNT, Amount::from_sat(1_000_000_000));
        assert_eq!(USER_TAKES_AFTER, 200);
        assert_eq!(
            UNSPENDABLE_XONLY_PUBKEY.to_string(),
            "93c7378d96518a75448821c4f7c8f4bae7ce60f804d03d1f0628dd5dd0f5de51"
        );
    }
}
