//! AES-256-GCM encryption with Argon2id key derivation
//!
//! This module provides authenticated encryption using AES-256-GCM with keys derived
//! from user passphrases via Argon2id KDF. Each encryption uses fresh salt and nonce
//! for semantic security.
//!
//! ## How It Works
//!
//! The encryption process combines two cryptographic primitives:
//!
//! 1. **Argon2id Key Derivation**: Transforms user passphrase into AES key
//! 2. **AES-256-GCM Encryption**: Provides authenticated encryption of data
//!
//! ### Complete Process Flow
//!
//! ```text
//! User Passphrase + Random Salt (32 bytes)
//!           ↓
//!    Argon2id KDF (memory-hard computation)
//!           ↓
//!   AES-256 Key (32 bytes)
//!           ↓
//! AES-256-GCM + Random Nonce (12 bytes)
//!           ↓
//!   Ciphertext + Authentication Tag
//! ```
//!
//! ## Argon2id Parameters
//!
//! Our implementation uses these specific parameters for key derivation:
//!
//! - **Time Cost: 3 iterations**
//!   - Computational rounds the algorithm performs
//!   - Higher values = more CPU time, better resistance to brute force
//!   - 3 is minimum recommended for interactive applications
//!
//! - **Memory Cost: 65536 KB (64 MB)**
//!   - Amount of RAM required during key derivation
//!   - Higher values = better resistance to GPU/ASIC attacks
//!   - 64MB provides strong security while remaining usable on most systems
//!
//! - **Parallelism: 1 thread**
//!   - Number of parallel threads during computation
//!   - Higher values can improve performance on multi-core systems
//!   - 1 thread ensures consistent timing across different hardware
//!
//! ### Parameter Trade-offs
//!
//! - **Security vs Performance**: Higher parameters = better security but slower operation
//! - **Memory vs Hardware Attacks**: More memory = harder for specialized hardware to attack
//! - **Time vs User Experience**: More iterations = stronger against brute force but slower UX
//!
//! ## Security Properties
//!
//! - **Semantic Security**: Same plaintext produces different ciphertext each time
//! - **Authentication**: AES-GCM detects any tampering with ciphertext
//! - **Key Diversity**: Each encryption uses unique salt → unique derived key
//! - **Memory Protection**: All sensitive data automatically zeroized after use
//! - **Forward Secrecy**: Compromised derived keys don't reveal original passphrase

use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use getrandom;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::{
    errors::BridgeCliError, passphrase::derive_key_from_passphrase, secure_structs::SecureString,
};

// Argon2id parameters: 3 iterations, 64MB memory, 1 thread
// Balances security (GPU/ASIC resistance) with interactive performance
const ARGON2_TIME_COST: u32 = 3;
const ARGON2_MEMORY_COST: u32 = 65536; // 64MB
const ARGON2_PARALLELISM: u32 = 1;

/// Complete encrypted data package: ciphertext + nonce + salt
#[derive(Debug, Clone)]
pub struct EncryptedData {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12], // AES-GCM nonce
    pub salt: [u8; 32],  // Argon2id salt
}

/// Hex-encoded version of EncryptedData for JSON serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedDataHex {
    pub ciphertext: String,
    pub nonce: String,
    pub salt: String,
}

/// Encrypts plaintext with AES-256-GCM using passphrase-derived key
///
/// Process: passphrase + fresh salt → Argon2id → AES key → GCM encryption with fresh nonce
pub fn aes_encrypt_secure(
    secure_plaintext: &SecureString,
    secure_passphrase: &SecureString,
) -> Result<EncryptedData, BridgeCliError> {
    // Generate fresh random salt and nonce
    let mut salt = [0u8; 32];
    let mut nonce_bytes = [0u8; 12];
    getrandom::fill(&mut salt).map_err(|e| BridgeCliError::RandomSaltGenerationError(e.to_string()))?;
    getrandom::fill(&mut nonce_bytes).map_err(|e| BridgeCliError::RandomNonceGenerationError(e.to_string()))?;

    // Derive AES key from passphrase + salt
    let secure_key = derive_key_from_passphrase(
        secure_passphrase,
        &salt,
        ARGON2_TIME_COST,
        ARGON2_MEMORY_COST,
        ARGON2_PARALLELISM,
    )
    .map_err(|e| BridgeCliError::KeyDerivationError(e.to_string()))?;

    // Encrypt with AES-256-GCM
    let cipher = Aes256Gcm::new(GenericArray::from_slice(secure_key.expose_secret()));
    let nonce = GenericArray::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, secure_plaintext.expose_secret().as_bytes())
        .map_err(|e| BridgeCliError::EncryptionError(e.to_string()))?;

    Ok(EncryptedData {
        ciphertext,
        nonce: nonce_bytes,
        salt,
    })
}

/// Decrypts AES-256-GCM ciphertext using passphrase-derived key
///
/// Process: passphrase + stored salt → Argon2id → same AES key → GCM decryption
pub fn aes_decrypt_secure(
    encrypted_data: &EncryptedData,
    secure_passphrase: &SecureString,
) -> Result<SecureString, BridgeCliError> {
    // Reconstruct same key using stored salt
    let secure_key = derive_key_from_passphrase(
        secure_passphrase,
        &encrypted_data.salt,
        ARGON2_TIME_COST,
        ARGON2_MEMORY_COST,
        ARGON2_PARALLELISM,
    )
    .map_err(|e| BridgeCliError::KeyDerivationError(e.to_string()))?;

    // Decrypt with AES-256-GCM (verifies authentication)
    let cipher = Aes256Gcm::new(GenericArray::from_slice(secure_key.expose_secret()));
    let nonce = GenericArray::from_slice(&encrypted_data.nonce);

    let mut plaintext = cipher
        .decrypt(nonce, encrypted_data.ciphertext.as_ref())
        .map_err(|e| BridgeCliError::DecryptionError(e.to_string()))?;

    let plaintext_string =
        String::from_utf8(plaintext.clone()).map_err(|e| BridgeCliError::InvalidUtf8Error(e.to_string()))?;

    let secure_string = SecureString::init_with(|| plaintext_string);
    plaintext.zeroize();

    Ok(secure_string)
}

/// Generic function to convert binary EncryptedData to hex format for JSON
pub fn encrypted_data_to_hex(data: &EncryptedData) -> EncryptedDataHex {
    EncryptedDataHex {
        ciphertext: hex::encode(&data.ciphertext),
        nonce: hex::encode(data.nonce),
        salt: hex::encode(data.salt),
    }
}

/// Generic function to convert hex EncryptedDataHex back to binary
pub fn encrypted_data_from_hex(data: &EncryptedDataHex) -> Result<EncryptedData, BridgeCliError> {
    Ok(EncryptedData {
        ciphertext: hex::decode(&data.ciphertext)?,
        nonce: hex::decode(&data.nonce)?
            .try_into()
            .map_err(|e: Vec<u8>| BridgeCliError::InvalidNonceLength(e.len()))?,
        salt: hex::decode(&data.salt)?
            .try_into()
            .map_err(|e: Vec<u8>| BridgeCliError::InvalidSaltLength(e.len()))?,
    })
}
