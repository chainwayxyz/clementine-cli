use bitcoin::{
    Address,
    address::{NetworkChecked, NetworkUnchecked, NetworkValidation},
    secp256k1::{Keypair, SecretKey},
};
use secrecy::SecretBox;
use serde::Deserialize;
use zeroize::Zeroize;

use crate::{
    BitcoinAddress,
    errors::BridgeCliError,
    wallet::{Purpose, address::parse_taproot_address},
};

pub(crate) type SecureString = SecretBox<String>;
pub(crate) type SecureByteSlice = SecretBox<[u8; 32]>;
pub(crate) type SecureByteVec = SecretBox<Vec<u8>>;
pub(crate) type SecureSeed = SecretBox<[u8; 64]>;

/// A secure wrapper for Vec<String> that automatically erases itself when dropped
pub(crate) struct SecureWordVec {
    inner: Vec<String>,
}

impl SecureWordVec {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn push(&mut self, word: String) {
        self.inner.push(word);
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn join(&self, separator: &str) -> String {
        self.inner.join(separator)
    }
}

impl Drop for SecureWordVec {
    fn drop(&mut self) {
        for word in &mut self.inner {
            word.zeroize();
        }
        self.inner.clear();
    }
}

/// A secure wrapper for SecretKey that automatically erases itself when dropped
pub struct SecureSecretKey {
    inner: SecretKey,
}

impl SecureSecretKey {
    pub fn new(key: SecretKey) -> Self {
        Self { inner: key }
    }

    pub fn as_ref_inner(&self) -> &SecretKey {
        &self.inner
    }
}

impl Drop for SecureSecretKey {
    fn drop(&mut self) {
        self.inner.non_secure_erase();
    }
}

/// A secure wrapper for Keypair that automatically erases itself when dropped
pub struct SecureKeypair {
    inner: Keypair,
}

impl SecureKeypair {
    pub fn new(keypair: Keypair) -> Self {
        Self { inner: keypair }
    }

    pub fn secret_key(&self) -> SecureSecretKey {
        SecureSecretKey::new(self.inner.secret_key())
    }
}

impl Drop for SecureKeypair {
    fn drop(&mut self) {
        self.inner.non_secure_erase();
    }
}

impl AsRef<Keypair> for SecureKeypair {
    fn as_ref(&self) -> &Keypair {
        &self.inner
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaprootAddressWithPrefix<T: NetworkValidation> {
    pub address: Address<T>,
    pub purpose: Purpose,
}

impl TaprootAddressWithPrefix<NetworkChecked> {
    pub fn new(address: Address<NetworkChecked>, purpose: Purpose) -> Result<Self, BridgeCliError> {
        let address_type = if let Some(t) = address.address_type() {
            t
        } else {
            return Err(BridgeCliError::InvalidAddressFormat);
        };

        if address_type != bitcoin::AddressType::P2tr {
            return Err(BridgeCliError::InvalidAddressFormat);
        }

        Ok(Self { address, purpose })
    }

    pub fn from_string_with_prefix(
        address: &str,
        network: bitcoin::Network,
    ) -> Result<Self, BridgeCliError> {
        if address.len() < 3 {
            return Err(BridgeCliError::InvalidAddressFormat);
        }

        let purpose = Purpose::purpose_from_str(&address[0..3]).map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!(
                "Failed to parse purpose from address: {} Error: {}",
                address,
                e
            ))
        })?;

        let addr_str = &address[3..];

        let bitcoin_address = parse_taproot_address(addr_str, network)?;

        let taproot_address_with_prefix = Self::new(bitcoin_address, purpose)?;

        Ok(taproot_address_with_prefix)
    }

    pub fn from_string_without_prefix(
        address: &str,
        purpose: Purpose,
        network: bitcoin::Network,
    ) -> Result<Self, BridgeCliError> {
        let bitcoin_address = parse_taproot_address(address, network)?;
        let taproot_address_with_prefix = Self::new(bitcoin_address, purpose)?;
        Ok(taproot_address_with_prefix)
    }
}

impl TaprootAddressWithPrefix<NetworkUnchecked> {
    pub fn from_string_with_prefix_unchecked(address: &str) -> Result<Self, BridgeCliError> {
        if address.len() < 4 {
            return Err(BridgeCliError::InvalidAddressFormat);
        }

        let purpose = Purpose::purpose_from_str(&address[0..3])?;
        let addr_str = &address[3..];

        let unchecked_address: BitcoinAddress<NetworkUnchecked> =
            addr_str.parse().map_err(|e| {
                BridgeCliError::Eyre(eyre::eyre!("Failed to parse Bitcoin address: {}", e))
            })?;

        let taproot_address_with_prefix = Self {
            address: unchecked_address,
            purpose,
        };

        Ok(taproot_address_with_prefix)
    }
}

pub trait AddrDisplay {
    fn as_display_str(&self) -> String;
}

impl AddrDisplay for Address<NetworkChecked> {
    fn as_display_str(&self) -> String {
        self.to_string()
    }
}
impl AddrDisplay for Address<NetworkUnchecked> {
    fn as_display_str(&self) -> String {
        self.clone().assume_checked().to_string()
    }
}

impl<T> TaprootAddressWithPrefix<T>
where
    T: NetworkValidation,
    Address<T>: AddrDisplay,
{
    pub fn address_without_prefix(&self) -> String {
        self.address.as_display_str()
    }

    pub fn address_with_prefix(&self) -> String {
        format!(
            "{}{}",
            self.purpose.to_prefix(),
            self.address_without_prefix()
        )
    }
}

#[derive(Debug, Deserialize)]
pub struct DepositStatus {
    pub id: u64,
    pub status: String,
    pub txid: String,
    pub evm_addr: String,
    pub move_txid: String,
    pub created_at: String,
    pub mint_txid: String,
}
