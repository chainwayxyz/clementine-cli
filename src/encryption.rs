use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce, aead::Aead};
use anyhow::anyhow;
use colored::Colorize;
use rand::{RngCore, rng};
use secrecy::ExposeSecret;
use zeroize::Zeroize;

use crate::{
    mnemonic::EncryptedData, passphrase::derive_key_from_passphrase, secure_structs::SecureString,
    wallet::CryptoParams,
};

/// Encrypt a private key with AES-256-GCM
pub fn encrypt_private_key(
    private_key: &str,
    passphrase: &SecureString,
) -> Result<CryptoParams, anyhow::Error> {
    println!("{} Encrypting private key...", "SECURE".green().bold());
    // Generate random salt and nonce
    let mut salt = [0u8; 32];
    let mut nonce_bytes = [0u8; 12];
    rng().fill_bytes(&mut salt);
    rng().fill_bytes(&mut nonce_bytes);
    println!("{} Generating salt and nonce...", "SECURE".green().bold());

    // Argon2id parameters (secure defaults)
    let iterations = 3; // 3 iterations
    let memory = 65_536; // 64 MB
    let parallelism = 4; // 4 threads

    println!("Generating encryption key...");

    // Derive encryption key
    let derived_key =
        derive_key_from_passphrase(passphrase, &salt, iterations, memory, parallelism)?;

    println!("Encryption key generated.");

    // Encrypt with AES-256-GCM
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(derived_key.expose_secret()));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, private_key.as_bytes())
        .map_err(|e| anyhow!("Encryption failed: {e}"))?;

    Ok(CryptoParams {
        kdf: "argon2id".to_string(),
        salt: hex::encode(salt),
        iterations,
        memory,
        parallelism,
        cipher: "aes-256-gcm".to_string(),
        nonce: hex::encode(nonce_bytes),
        ciphertext: hex::encode(ciphertext),
    })
}

/// Decrypt a private key with AES-256-GCM
pub fn decrypt_private_key(
    crypto: &CryptoParams,
    passphrase: &SecureString,
) -> Result<SecureString, anyhow::Error> {
    // Validate crypto parameters
    if crypto.kdf != "argon2id" {
        return Err(anyhow!("Unsupported KDF: {}", crypto.kdf));
    }
    if crypto.cipher != "aes-256-gcm" {
        return Err(anyhow!("Unsupported cipher: {}", crypto.cipher));
    }

    // Decode hex values
    let salt = hex::decode(&crypto.salt)?;
    let nonce_bytes = hex::decode(&crypto.nonce)?;
    let ciphertext = hex::decode(&crypto.ciphertext)?;

    // Derive decryption key
    let derived_key = derive_key_from_passphrase(
        passphrase,
        &salt,
        crypto.iterations,
        crypto.memory,
        crypto.parallelism,
    )?;

    // Decrypt with AES-256-GCM
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(derived_key.expose_secret()));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| anyhow!("Decryption failed: invalid passphrase or corrupted data"))?;

    let private_key_str =
        String::from_utf8(plaintext).map_err(|_| anyhow!("Decryption failed: invalid UTF-8"))?;
    let secure_private_key = SecureString::init_with(|| private_key_str);

    Ok(secure_private_key)
}

pub fn aes_encrypt_secure(
    secure_plaintext: &SecureString,
    secure_passphrase: &SecureString,
) -> Result<EncryptedData, anyhow::Error> {
    let mut salt = [0u8; 32];
    let mut nonce_bytes = [0u8; 12];
    rng().fill_bytes(&mut salt);
    rng().fill_bytes(&mut nonce_bytes);

    let secure_key = derive_key_from_passphrase(&secure_passphrase, &salt, 3, 65536, 1)
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
    let secure_key =
        derive_key_from_passphrase(&secure_passphrase, &encrypted_data.salt, 3, 65536, 1)
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
