use anyhow::Result;
use bitcoin::XOnlyPublicKey;
use bitcoin::hashes::{Hash as _, sha256};
use bitcoin::secp256k1::{PublicKey, SecretKey};
use secp256k1::SECP256K1;
use secp256k1::musig::KeyAggCache;

/// Create an aggregated XOnlyPublicKey from multiple public keys using MuSig2
pub fn from_musig2_pks(pks: Vec<PublicKey>) -> Result<XOnlyPublicKey> {
    let musig_key_agg_cache = create_key_agg_cache(pks)?;

    Ok(
        XOnlyPublicKey::from_slice(&musig_key_agg_cache.agg_pk().serialize())
            .expect("Failed to create XOnlyPublicKey from aggregated public key"),
    )
}

/// Create a MuSig2 key aggregation cache from public keys
pub fn create_key_agg_cache(mut public_keys: Vec<PublicKey>) -> Result<KeyAggCache> {
    public_keys.sort();
    let secp_pubkeys: Vec<secp256k1::PublicKey> = public_keys
        .iter()
        .map(|pk| {
            secp256k1::PublicKey::from_slice(&pk.serialize()).expect("serialized pubkey is valid")
        })
        .collect();
    let pubkeys_ref: Vec<&secp256k1::PublicKey> = secp_pubkeys.iter().collect();
    let pubkeys_ref = pubkeys_ref.as_slice();

    let musig_key_agg_cache = KeyAggCache::new(SECP256K1, pubkeys_ref);

    Ok(musig_key_agg_cache)
}

/// Get the aggregated N-of-N XOnlyPublicKey for test verifiers
pub fn get_nofn_xonly_pk(n_verifiers: usize) -> Result<XOnlyPublicKey> {
    let verifiers_secret_keys = (0..n_verifiers)
        .map(|i| {
            SecretKey::from_slice(&seeded_key("verifier", i as u8))
                .expect("failed to create secret key")
        })
        .collect::<Vec<_>>();

    let secp = bitcoin::secp256k1::Secp256k1::new();
    let verifiers_public_keys: Vec<PublicKey> = verifiers_secret_keys
        .iter()
        .map(|sk| PublicKey::from_secret_key(&secp, sk))
        .collect();

    from_musig2_pks(verifiers_public_keys)
}

/// Get default bridge initialization parameters for testing
pub fn get_default_bridge_params(n_verifiers: usize) -> String {
    let nofn_xonly_pk = get_nofn_xonly_pk(n_verifiers).expect("Failed to get nofn xonly pk");

    let expected_len = "000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000008ac7230489e80000000000000000000000000000000000000000000000000000000000000000002d4a209fb3a961d8b1f4ec1caa220c6a50b815febc0b689ddf0b9ddfbf99cb74479e41ac0063066369747265611400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a08000000003b9aca006800000000000000000000000000000000000000000000".len();

    let params = format!(
        "000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000008ac7230489e80000000000000000000000000000000000000000000000000000000000000000002d4120{}ac006306636974726561140000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000016800000000000000000000000000000000000000000000000000000000000000",
        nofn_xonly_pk
    );

    assert_eq!(params.len(), expected_len);

    params
}

/// Generate a deterministic key for testing purposes
pub fn seeded_key(prefix: &str, idx: u8) -> [u8; 32] {
    sha256::Hash::hash(format!("{prefix}-{idx}").as_bytes()).to_byte_array()
}
