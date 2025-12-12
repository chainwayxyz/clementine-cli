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

use crate::secure_types::{SecureByteVec, SecureString};
use crate::{errors::BridgeCliError, wallet::passphrase::derive_key_from_passphrase};
use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};

// Argon2id parameters: 3 iterations, 64MB memory, 1 thread
// Balances security (GPU/ASIC resistance) with interactive performance
const ARGON2_TIME_COST: u32 = 3;
const ARGON2_MEMORY_COST: u32 = 65536; // 64MB
const ARGON2_PARALLELISM: u32 = 1;

/// Complete encrypted data package: ciphertext + nonce + salt
#[derive(Debug, Clone)]
pub(crate) struct EncryptedData {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12], // AES-GCM nonce
    pub salt: [u8; 32],  // Argon2id salt
}

/// Hex-encoded version of EncryptedData for JSON serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EncryptedDataHex {
    pub ciphertext: String,
    pub nonce: String,
    pub salt: String,
}

/// Encrypts plaintext with AES-256-GCM using passphrase-derived key
///
/// Process: passphrase + fresh salt → Argon2id → AES key → GCM encryption with fresh nonce
pub(crate) fn aes_encrypt_secure(
    secure_plaintext: &SecureString,
    secure_passphrase: &SecureString,
) -> Result<EncryptedData, BridgeCliError> {
    // Generate fresh random salt and nonce
    let mut salt = [0u8; 32];
    let mut nonce_bytes = [0u8; 12];
    getrandom::fill(&mut salt).map_err(|e| {
        tracing::error!("Error generating random salt: {}", e);
        BridgeCliError::RandomSaltGenerationError
    })?;

    getrandom::fill(&mut nonce_bytes).map_err(|e| {
        tracing::error!("Error generating random nonce: {}", e);
        BridgeCliError::RandomNonceGenerationError
    })?;

    // Derive AES key from passphrase + salt
    let secure_key = derive_key_from_passphrase(
        secure_passphrase,
        &salt,
        ARGON2_TIME_COST,
        ARGON2_MEMORY_COST,
        ARGON2_PARALLELISM,
    )
    .map_err(|e| {
        tracing::error!("Error deriving key from passphrase: {}", e);
        BridgeCliError::EncryptionKeyDerivationError
    })?;

    // Encrypt with AES-256-GCM
    let cipher = Aes256Gcm::new(GenericArray::from_slice(secure_key.expose_secret()));
    let nonce = GenericArray::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, secure_plaintext.expose_secret().as_bytes())
        .map_err(|e| {
            tracing::error!("Error encrypting data: {}", e);
            BridgeCliError::EncryptionError
        })?;

    Ok(EncryptedData {
        ciphertext,
        nonce: nonce_bytes,
        salt,
    })
}

/// Decrypts AES-256-GCM ciphertext using passphrase-derived key
///
/// Process: passphrase + stored salt → Argon2id → same AES key → GCM decryption
pub(crate) fn aes_decrypt_secure(
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
    .map_err(|e| {
        tracing::error!("Error deriving key from passphrase: {}", e);
        BridgeCliError::EncryptionKeyDerivationError
    })?;

    // Decrypt with AES-256-GCM (verifies authentication)
    let cipher = Aes256Gcm::new(GenericArray::from_slice(secure_key.expose_secret()));
    let nonce = GenericArray::from_slice(&encrypted_data.nonce);

    let plaintext = cipher
        .decrypt(nonce, encrypted_data.ciphertext.as_ref())
        .map_err(|e| {
            tracing::error!("Error decrypting data: {}", e);
            BridgeCliError::DecryptionError
        })?;

    let secure_plaintext_bytes = SecureByteVec::new(Box::new(plaintext));

    let plaintext_string = String::from_utf8(secure_plaintext_bytes.expose_secret().clone())
        .map_err(|e| {
            tracing::error!("Error converting plaintext to UTF-8: {}", e);
            BridgeCliError::InvalidUtf8Error
        })?;

    let secure_string = SecureString::init_with(|| plaintext_string);

    Ok(secure_string)
}

/// Generic function to convert binary EncryptedData to hex format for JSON
pub(crate) fn encrypted_data_to_hex(data: &EncryptedData) -> EncryptedDataHex {
    EncryptedDataHex {
        ciphertext: hex::encode(&data.ciphertext),
        nonce: hex::encode(data.nonce),
        salt: hex::encode(data.salt),
    }
}

/// Generic function to convert hex EncryptedDataHex back to binary
pub(crate) fn encrypted_data_from_hex(
    data: &EncryptedDataHex,
) -> Result<EncryptedData, BridgeCliError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PASSPHRASE: &str = "test_passphrase_12345";

    #[test]
    fn test_encrypt_decrypt_round_trip() {
        let plaintext = SecureString::init_with(|| "sensitive wallet data".to_string());
        let passphrase = SecureString::init_with(|| TEST_PASSPHRASE.to_string());

        // Encrypt
        let encrypted =
            aes_encrypt_secure(&plaintext, &passphrase).expect("Encryption should succeed");

        // Decrypt
        let decrypted =
            aes_decrypt_secure(&encrypted, &passphrase).expect("Decryption should succeed");

        assert_eq!(plaintext.expose_secret(), decrypted.expose_secret());
    }

    #[test]
    fn test_decrypt_with_wrong_passphrase_fails() {
        let plaintext = SecureString::init_with(|| "sensitive data".to_string());
        let passphrase1 = SecureString::init_with(|| "correct_pass".to_string());
        let passphrase2 = SecureString::init_with(|| "wrong_pass".to_string());

        let encrypted =
            aes_encrypt_secure(&plaintext, &passphrase1).expect("Encryption should succeed");

        let result = aes_decrypt_secure(&encrypted, &passphrase2);
        assert!(
            result.is_err(),
            "Decryption with wrong passphrase should fail"
        );

        // Verify it's a decryption error
        match result {
            Err(BridgeCliError::DecryptionError) => (),
            _ => panic!("Expected DecryptionError or EncryptionKeyDerivationError"),
        }
    }

    #[test]
    fn test_encrypted_data_to_from_hex_round_trip() {
        let plaintext = SecureString::init_with(|| "test data".to_string());
        let passphrase = SecureString::init_with(|| TEST_PASSPHRASE.to_string());

        let encrypted =
            aes_encrypt_secure(&plaintext, &passphrase).expect("Encryption should succeed");

        // Convert to hex
        let hex_data = encrypted_data_to_hex(&encrypted);

        // Verify hex strings are valid
        assert!(!hex_data.ciphertext.is_empty());
        assert!(!hex_data.nonce.is_empty());
        assert!(!hex_data.salt.is_empty());

        // Convert back from hex
        let recovered = encrypted_data_from_hex(&hex_data).expect("Should parse from hex");

        // Verify decryption still works
        let decrypted =
            aes_decrypt_secure(&recovered, &passphrase).expect("Should decrypt recovered data");

        assert_eq!(plaintext.expose_secret(), decrypted.expose_secret());
    }

    #[test]
    fn test_encrypted_data_from_hex_invalid_hex() {
        let invalid_hex = EncryptedDataHex {
            ciphertext: "not_valid_hex_zzz".to_string(),
            nonce: "aabbccdd".to_string(),
            salt: "11223344".to_string(),
        };

        let result = encrypted_data_from_hex(&invalid_hex);
        assert!(result.is_err(), "Should fail on invalid hex");
    }

    #[test]
    fn test_encrypted_data_from_hex_invalid_nonce_length() {
        let invalid_nonce = EncryptedDataHex {
            ciphertext: hex::encode(vec![1, 2, 3, 4]),
            nonce: hex::encode(vec![1, 2, 3]), // Wrong length (should be 12)
            salt: hex::encode(vec![0u8; 32]),
        };

        let result = encrypted_data_from_hex(&invalid_nonce);
        assert!(result.is_err(), "Should fail on invalid nonce length");

        match result {
            Err(BridgeCliError::InvalidNonceLength(len)) => {
                assert_eq!(len, 3, "Should report actual length");
            }
            _ => panic!("Expected InvalidNonceLength error"),
        }
    }

    #[test]
    fn test_encrypted_data_from_hex_invalid_salt_length() {
        let invalid_salt = EncryptedDataHex {
            ciphertext: hex::encode(vec![1, 2, 3, 4]),
            nonce: hex::encode(vec![0u8; 12]),
            salt: hex::encode(vec![1, 2, 3]), // Wrong length (should be 32)
        };

        let result = encrypted_data_from_hex(&invalid_salt);
        assert!(result.is_err(), "Should fail on invalid salt length");

        match result {
            Err(BridgeCliError::InvalidSaltLength(len)) => {
                assert_eq!(len, 3, "Should report actual length");
            }
            _ => panic!("Expected InvalidSaltLength error"),
        }
    }

    #[test]
    fn test_encryption_produces_different_ciphertext() {
        // Same plaintext + passphrase should produce different ciphertext due to random nonce/salt
        let plaintext = SecureString::init_with(|| "test data".to_string());
        let passphrase = SecureString::init_with(|| TEST_PASSPHRASE.to_string());

        let encrypted1 =
            aes_encrypt_secure(&plaintext, &passphrase).expect("First encryption should succeed");
        let encrypted2 =
            aes_encrypt_secure(&plaintext, &passphrase).expect("Second encryption should succeed");

        // Ciphertexts should be different
        assert_ne!(
            encrypted1.ciphertext, encrypted2.ciphertext,
            "Different encryptions should produce different ciphertext"
        );

        // Nonces should be different
        assert_ne!(
            encrypted1.nonce, encrypted2.nonce,
            "Different encryptions should use different nonces"
        );

        // Salts should be different
        assert_ne!(
            encrypted1.salt, encrypted2.salt,
            "Different encryptions should use different salts"
        );

        // But both should decrypt to same plaintext
        let decrypted1 = aes_decrypt_secure(&encrypted1, &passphrase).unwrap();
        let decrypted2 = aes_decrypt_secure(&encrypted2, &passphrase).unwrap();
        assert_eq!(decrypted1.expose_secret(), decrypted2.expose_secret());
    }

    #[test]
    fn test_nonce_has_correct_length() {
        let plaintext = SecureString::init_with(|| "test".to_string());
        let passphrase = SecureString::init_with(|| TEST_PASSPHRASE.to_string());

        let encrypted =
            aes_encrypt_secure(&plaintext, &passphrase).expect("Encryption should succeed");

        assert_eq!(
            encrypted.nonce.len(),
            12,
            "Nonce should be 12 bytes for AES-GCM"
        );
    }

    #[test]
    fn test_salt_has_correct_length() {
        let plaintext = SecureString::init_with(|| "test".to_string());
        let passphrase = SecureString::init_with(|| TEST_PASSPHRASE.to_string());

        let encrypted =
            aes_encrypt_secure(&plaintext, &passphrase).expect("Encryption should succeed");

        assert_eq!(encrypted.salt.len(), 32, "Salt should be 32 bytes");
    }

    #[test]
    fn test_different_passphrases_produce_different_results() {
        let plaintext = SecureString::init_with(|| "test data".to_string());
        let passphrase1 = SecureString::init_with(|| "password1".to_string());
        let passphrase2 = SecureString::init_with(|| "password2".to_string());

        let encrypted1 =
            aes_encrypt_secure(&plaintext, &passphrase1).expect("First encryption should succeed");
        let encrypted2 =
            aes_encrypt_secure(&plaintext, &passphrase2).expect("Second encryption should succeed");

        // Different passphrases should produce different ciphertexts
        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);

        // Each should decrypt with its own passphrase
        let decrypted1 =
            aes_decrypt_secure(&encrypted1, &passphrase1).expect("Should decrypt with passphrase1");
        let decrypted2 =
            aes_decrypt_secure(&encrypted2, &passphrase2).expect("Should decrypt with passphrase2");

        // But not with the wrong passphrase
        assert!(aes_decrypt_secure(&encrypted1, &passphrase2).is_err());
        assert!(aes_decrypt_secure(&encrypted2, &passphrase1).is_err());

        // Both should produce the same plaintext
        assert_eq!(decrypted1.expose_secret(), decrypted2.expose_secret());
    }
}
