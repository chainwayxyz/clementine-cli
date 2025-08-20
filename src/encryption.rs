use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use anyhow::anyhow;
use rand::{RngCore, rng};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::{passphrase::derive_key_from_passphrase, secure_structs::SecureString};

// Argon2 parameters for key derivation
const ARGON2_TIME_COST: u32 = 3; // Number of iterations
const ARGON2_MEMORY_COST: u32 = 65536; // Memory usage in KB (64 MB)
const ARGON2_PARALLELISM: u32 = 1; // Number of parallel threads

/// Generic encrypted data structure for binary data
#[derive(Debug, Clone)]
pub struct EncryptedData {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12], // AES-GCM standard nonce size
    pub salt: [u8; 32],  // Salt for key derivation
}

/// Hex-encoded version of EncryptedData for JSON serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedDataHex {
    pub ciphertext: String,
    pub nonce: String,
    pub salt: String,
}

pub fn aes_encrypt_secure(
    secure_plaintext: &SecureString,
    secure_passphrase: &SecureString,
) -> Result<EncryptedData, anyhow::Error> {
    let mut salt = [0u8; 32];
    let mut nonce_bytes = [0u8; 12];
    rng().fill_bytes(&mut salt);
    rng().fill_bytes(&mut nonce_bytes);

    let secure_key = derive_key_from_passphrase(
        &secure_passphrase,
        &salt,
        ARGON2_TIME_COST,
        ARGON2_MEMORY_COST,
        ARGON2_PARALLELISM,
    )
    .map_err(|e| anyhow!("Key derivation failed: {}", e))?;

    let cipher = Aes256Gcm::new(GenericArray::from_slice(secure_key.expose_secret()));
    let nonce = GenericArray::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, secure_plaintext.expose_secret().as_bytes())
        .map_err(|e| anyhow!("Encryption failed: {}", e))?;

    Ok(EncryptedData {
        ciphertext,
        nonce: nonce_bytes,
        salt,
    })
}

pub fn aes_decrypt_secure(
    encrypted_data: &EncryptedData,
    secure_passphrase: &SecureString,
) -> Result<SecureString, anyhow::Error> {
    let secure_key = derive_key_from_passphrase(
        &secure_passphrase,
        &encrypted_data.salt,
        ARGON2_TIME_COST,
        ARGON2_MEMORY_COST,
        ARGON2_PARALLELISM,
    )
    .map_err(|e| anyhow!("Key derivation failed: {}", e))?;

    let cipher = Aes256Gcm::new(GenericArray::from_slice(secure_key.expose_secret()));
    let nonce = GenericArray::from_slice(&encrypted_data.nonce);

    let mut plaintext = cipher
        .decrypt(nonce, encrypted_data.ciphertext.as_ref())
        .map_err(|e| anyhow!("Decryption failed: {}", e))?;

    let plaintext_string = String::from_utf8(plaintext.clone())
        .map_err(|_| anyhow!("Decryption produced invalid UTF-8"))?;

    let secure_string = SecureString::init_with(|| plaintext_string);

    plaintext.zeroize();

    Ok(secure_string)
}

/// Generic function to convert binary EncryptedData to hex format for JSON
pub fn encrypted_data_to_hex(data: &EncryptedData) -> EncryptedDataHex {
    EncryptedDataHex {
        ciphertext: hex::encode(&data.ciphertext),
        nonce: hex::encode(&data.nonce),
        salt: hex::encode(&data.salt),
    }
}

/// Generic function to convert hex EncryptedDataHex back to binary
pub fn encrypted_data_from_hex(data: &EncryptedDataHex) -> Result<EncryptedData, anyhow::Error> {
    Ok(EncryptedData {
        ciphertext: hex::decode(&data.ciphertext)?,
        nonce: hex::decode(&data.nonce)?
            .try_into()
            .map_err(|_| anyhow!("Invalid nonce length"))?,
        salt: hex::decode(&data.salt)?
            .try_into()
            .map_err(|_| anyhow!("Invalid salt length"))?,
    })
}
