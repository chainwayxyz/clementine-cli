use secrecy::SecretBox;
use bitcoin::secp256k1::{SecretKey, Keypair};
use bip39::Mnemonic;
use zeroize::Zeroize;

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

/// A secure wrapper for Mnemonic that automatically erases itself when dropped
pub(crate) struct SecureMnemonic {
    inner: Mnemonic,
}

impl SecureMnemonic {
    pub fn new(mnemonic: Mnemonic) -> Self {
        Self { inner: mnemonic }
    }

    pub fn as_ref(&self) -> &Mnemonic {
        &self.inner
    }

    pub fn to_seed(&self, passphrase: &str) -> SecureSeed {
        SecureSeed::new(Box::new(self.inner.to_seed(passphrase)))
    }
}

impl Drop for SecureMnemonic {
    fn drop(&mut self) {
        self.inner.zeroize();
    }
}

/// A secure wrapper for SecretKey that automatically erases itself when dropped
pub(crate) struct SecureSecretKey {
    inner: SecretKey,
}

impl SecureSecretKey {
    pub fn new(key: SecretKey) -> Self {
        Self { inner: key }
    }

    pub fn as_ref(&self) -> &SecretKey {
        &self.inner
    }

}

impl Drop for SecureSecretKey {
    fn drop(&mut self) {
        self.inner.non_secure_erase();
    }
}

/// A secure wrapper for Keypair that automatically erases itself when dropped
pub(crate) struct SecureKeypair {
    inner: Keypair,
}

impl SecureKeypair {
    pub fn new(keypair: Keypair) -> Self {
        Self { inner: keypair }
    }

    pub fn as_ref(&self) -> &Keypair {
        &self.inner
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

pub(crate) trait AddressExt {
    fn is_taproot(&self) -> bool;
}

impl AddressExt for bitcoin::Address {
    fn is_taproot(&self) -> bool {
        self.address_type() == Some(bitcoin::AddressType::P2tr)
    }
}
