use bitcoin::secp256k1::{Keypair, SecretKey};
use secrecy::SecretBox;
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
