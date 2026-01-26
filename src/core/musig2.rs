//! # MuSig2
//!
//! Helper functions for the MuSig2 signature scheme.

use bitcoin::XOnlyPublicKey;
use bitcoin::secp256k1::PublicKey;
use secp256k1::{SECP256K1, musig::KeyAggCache};

use super::errors::BridgeCliError;

/// Convert a bitcoin::secp256k1::PublicKey to secp256k1::PublicKey
fn to_secp_pk(pk: PublicKey) -> secp256k1::PublicKey {
    secp256k1::PublicKey::from_slice(&pk.serialize()).expect("serialized pubkey is valid")
}

/// Create a MuSig2 key aggregation cache from a list of public keys
fn create_key_agg_cache(public_keys: &[PublicKey]) -> KeyAggCache {
    let mut public_keys = public_keys.to_vec();
    public_keys.sort();
    let secp_pubkeys: Vec<secp256k1::PublicKey> =
        public_keys.iter().map(|pk| to_secp_pk(*pk)).collect();
    let pubkeys_ref: Vec<&secp256k1::PublicKey> = secp_pubkeys.iter().collect();

    KeyAggCache::new(SECP256K1, &pubkeys_ref)
}

/// Aggregate multiple public keys using MuSig2 key aggregation
pub fn aggregate_public_keys(pks: &[PublicKey]) -> Result<XOnlyPublicKey, BridgeCliError> {
    if pks.is_empty() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "No public keys provided for aggregation"
        )));
    }
    if pks.len() == 1 {
        return Ok(pks[0].x_only_public_key().0);
    }

    let musig_key_agg_cache = create_key_agg_cache(pks);

    XOnlyPublicKey::from_slice(&musig_key_agg_cache.agg_pk().serialize()).map_err(|e| {
        tracing::error!("Failed to create XOnlyPublicKey from aggregated key: {}", e);
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to create XOnlyPublicKey from aggregated public key"
        ))
    })
}

/// Parse a comma-separated list of hex-encoded public keys and aggregate them
pub fn aggregate_public_keys_from_str(
    public_keys_str: &str,
) -> Result<XOnlyPublicKey, BridgeCliError> {
    let public_keys: Result<Vec<PublicKey>, _> = public_keys_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|hex_str| {
            let bytes = hex::decode(hex_str).map_err(|e| {
                tracing::error!("Failed to decode public key hex '{}': {}", hex_str, e);
                BridgeCliError::Eyre(eyre::eyre!(
                    "Failed to decode public key hex '{}': {}",
                    hex_str,
                    e
                ))
            })?;
            PublicKey::from_slice(&bytes).map_err(|e| {
                tracing::error!("Failed to parse public key '{}': {}", hex_str, e);
                BridgeCliError::Eyre(eyre::eyre!(
                    "Failed to parse public key '{}': {}",
                    hex_str,
                    e
                ))
            })
        })
        .collect();

    aggregate_public_keys(&public_keys?)
}
