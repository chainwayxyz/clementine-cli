use std::str::FromStr;

use bitcoin::{
    Network,
    bip32::{DerivationPath, Xpriv},
    secp256k1::SecretKey,
};

pub fn derive_private_key(
    master_seed: &[u8; 32],
    derivation_path: &str,
    network: Network,
) -> Result<SecretKey, anyhow::Error> {
    let master_xpriv = Xpriv::new_master(network, master_seed)?;

    let path = DerivationPath::from_str(derivation_path)?;

    let child_xpriv = master_xpriv.derive_priv(&crate::bitcoin_utils::SECP, &path)?;

    Ok(child_xpriv.private_key)
}
