//! # MuSig2
//!
//! Helper functions for the MuSig2 signature scheme.

use bitcoin::{XOnlyPublicKey, secp256k1::PublicKey};
use eyre::Context;
use secp256k1::{
    SECP256K1,
    musig::{KeyAggCache, PublicNonce, SecretNonce},
};

pub type MuSigNoncePair = (SecretNonce, PublicNonce);

pub fn from_secp_xonly(xpk: secp256k1::XOnlyPublicKey) -> XOnlyPublicKey {
    XOnlyPublicKey::from_slice(&xpk.serialize()).expect("serialized pubkey is valid")
}

pub fn to_secp_pk(pk: PublicKey) -> secp256k1::PublicKey {
    secp256k1::PublicKey::from_slice(&pk.serialize()).expect("serialized pubkey is valid")
}
pub fn from_secp_pk(pk: secp256k1::PublicKey) -> PublicKey {
    PublicKey::from_slice(&pk.serialize()).expect("serialized pubkey is valid")
}

fn create_key_agg_cache(public_keys: &[PublicKey]) -> eyre::Result<KeyAggCache> {
    let mut public_keys = public_keys.to_vec();
    public_keys.sort();
    let secp_pubkeys: Vec<secp256k1::PublicKey> =
        public_keys.iter().map(|pk| to_secp_pk(*pk)).collect();
    let pubkeys_ref: Vec<&secp256k1::PublicKey> = secp_pubkeys.iter().collect();
    let pubkeys_ref = pubkeys_ref.as_slice();

    let musig_key_agg_cache = KeyAggCache::new(SECP256K1, pubkeys_ref);

    Ok(musig_key_agg_cache)
}

pub trait AggregateFromPublicKeys {
    fn from_musig2_pks(pks: &[PublicKey]) -> eyre::Result<XOnlyPublicKey>;
}

impl AggregateFromPublicKeys for XOnlyPublicKey {
    fn from_musig2_pks(pks: &[PublicKey]) -> eyre::Result<XOnlyPublicKey> {
        if pks.is_empty() {
            return Err(eyre::eyre!("No public keys provided"));
        }
        if pks.len() == 1 {
            return Ok(pks[0].x_only_public_key().0);
        }
        let musig_key_agg_cache = create_key_agg_cache(pks)?;

        XOnlyPublicKey::from_slice(&musig_key_agg_cache.agg_pk().serialize())
            .wrap_err("Failed to create XOnlyPublicKey from aggregated public key")
    }
}
