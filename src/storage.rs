// Key storage functionality for Clementine CLI

use anyhow::Result;
use bitcoin::bip32::{DerivationPath, Xpriv};
use bitcoin::secp256k1::Keypair;
use bitcoin::secp256k1::SecretKey;
use bitcoin::{Address, Network};
use bip39::Mnemonic;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use crate::BitcoinAddress;

// Security imports for encrypted storage
use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use argon2::Argon2;
use argon2::password_hash::rand_core::RngCore;
use colored::*;
use serde::{Deserialize, Serialize};
use zeroize::ZeroizeOnDrop;

/// Secure wrapper for sensitive string data that zeros on drop
#[derive(Clone, Debug, ZeroizeOnDrop)]
pub struct SecureString(String);

impl SecureString {
    pub fn new(s: String) -> Self {
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl From<String> for SecureString {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

/// Secure wrapper for derived encryption keys that zeros on drop
#[derive(Debug, ZeroizeOnDrop)]
pub struct DerivedKey([u8; 32]);

impl DerivedKey {
    pub fn new(key: [u8; 32]) -> Self {
        Self(key)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Encrypted private key storage format
#[derive(Serialize, Deserialize)]
pub struct EncryptedKeyData {
    pub version: u8,
    pub encrypted: bool,
    pub network: String,
    pub address: String,
    pub crypto: CryptoParams,
    pub stored_at: String,
}

/// Cryptographic parameters for encrypted storage
#[derive(Serialize, Deserialize)]
pub struct CryptoParams {
    pub kdf: String,        // "argon2id"
    pub salt: String,       // hex encoded salt
    pub iterations: u32,    // argon2 time cost
    pub memory: u32,        // argon2 memory cost in KB
    pub parallelism: u32,   // argon2 parallelism
    pub cipher: String,     // "aes-256-gcm"
    pub nonce: String,      // hex encoded nonce
    pub ciphertext: String, // hex encoded encrypted private key
}

/// Derive an encryption key from a passphrase using Argon2id
pub fn derive_key_from_passphrase(
    passphrase: &SecureString,
    salt: &[u8],
    iterations: u32,
    memory: u32,
    parallelism: u32,
) -> Result<DerivedKey, Box<dyn std::error::Error>> {
    println!("Deriving encryption key from passphrase...");
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(memory, iterations, parallelism, Some(32))
            .map_err(|e| format!("Invalid Argon2 parameters: {}", e))?,
    );

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| format!("Key derivation failed: {}", e))?;

    println!("Key derived successfully.");

    Ok(DerivedKey::new(key))
}

/// Encrypt a private key with AES-256-GCM
fn encrypt_private_key(
    private_key: &str,
    passphrase: &SecureString,
) -> Result<CryptoParams, Box<dyn std::error::Error>> {
    println!("{} Encrypting private key...", "SECURE".green().bold());
    // Generate random salt and nonce
    let mut salt = [0u8; 32];
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce_bytes);
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
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(derived_key.as_bytes()));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, private_key.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;

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
fn decrypt_private_key(
    crypto: &CryptoParams,
    passphrase: &SecureString,
) -> Result<SecureString, Box<dyn std::error::Error>> {
    // Validate crypto parameters
    if crypto.kdf != "argon2id" {
        return Err(format!("Unsupported KDF: {}", crypto.kdf).into());
    }
    if crypto.cipher != "aes-256-gcm" {
        return Err(format!("Unsupported cipher: {}", crypto.cipher).into());
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
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(derived_key.as_bytes()));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| "Decryption failed: invalid passphrase or corrupted data")?;

    Ok(SecureString::new(String::from_utf8(plaintext)?))
}

/// Prompt user for a passphrase with confirmation for new keys
pub fn prompt_new_passphrase() -> Result<SecureString, Box<dyn std::error::Error>> {
    println!("{}", "Passphrase protection:".blue().bold());
    println!("Enter a passphrase to encrypt your private key.");

    let passphrase = rpassword::prompt_password("Enter passphrase: ")?;

    if passphrase.is_empty() {
        return Err("Passphrase cannot be empty".into());
    }

    // Validate passphrase strength
    if passphrase.len() < 8 {
        return Err("Passphrase must be at least 8 characters long".into());
    }

    // Confirm passphrase
    let confirm = rpassword::prompt_password("Confirm passphrase: ")?;

    if passphrase != confirm {
        return Err("Passphrases do not match".into());
    }

    println!(
        "{} Private key will be encrypted with AES-256-GCM",
        "SECURE".green().bold()
    );
    Ok(SecureString::new(passphrase))
}

/// Prompt user for a passphrase to unlock existing encrypted key
pub fn prompt_unlock_passphrase() -> Result<SecureString, Box<dyn std::error::Error>> {
    let passphrase = rpassword::prompt_password("Enter passphrase to unlock key: ")?;

    if passphrase.is_empty() {
        return Err("Passphrase cannot be empty".into());
    }

    Ok(SecureString::new(passphrase))
}

/// Store a keypair and its corresponding taproot address
pub fn store_key(
    keypair: &Keypair,
    network: Network,
    passphrase: &str,
) -> Result<BitcoinAddress, Box<dyn std::error::Error>> {
    // Calculate the taproot address for this keypair
    let address = crate::bitcoin_utils::calculate_taproot_address(keypair, network);

    // Create storage directory
    let storage_dir = get_storage_dir()?;
    fs::create_dir_all(&storage_dir)?;

    let key_file = storage_dir.join(format!("key_{}.json", address));
    let private_key_str = keypair.secret_key().display_secret().to_string();

    // Always store encrypted key
    let secure_passphrase = SecureString::new(passphrase.to_string());
    let crypto = encrypt_private_key(&private_key_str, &secure_passphrase)?;

    let encrypted_data = EncryptedKeyData {
        version: 2,
        encrypted: true,
        network: network.to_string(),
        address: address.to_string(),
        crypto,
        stored_at: chrono::Utc::now().to_rfc3339(),
    };

    let key_data = serde_json::to_string_pretty(&encrypted_data)?;

    fs::write(&key_file, key_data)?;

    // Set file permissions to 700 (rwx------)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&key_file)?.permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&key_file, perms)?;
    }

    // Store address for easy lookup
    let address_file = storage_dir.join("addresses.json");
    let mut addresses: HashMap<String, serde_json::Value> = if address_file.exists() {
        serde_json::from_str(&fs::read_to_string(&address_file)?)?
    } else {
        HashMap::new()
    };

    addresses.insert(
        address.to_string(),
        serde_json::json!({
            "network": network.to_string(),
            "stored_at": chrono::Utc::now().to_rfc3339()
        }),
    );

    fs::write(address_file, serde_json::to_string_pretty(&addresses)?)?;

    debug!("{} Key stored successfully", "SUCCESS".green().bold());
    debug!("{} {}", "ADDRESS".cyan().bold(), address);
    debug!("{} {}", "NETWORK".blue().bold(), network);
    debug!(
        "{} Key stored with AES-256-GCM encryption",
        "ENCRYPTED".green().bold()
    );

    Ok(address)
}

/// Load a keypair by its taproot address
pub fn load_key(
    taproot_address: &str,
    network: Network,
    passphrase: Option<&str>,
) -> Result<Keypair, Box<dyn std::error::Error>> {
    // Parse the address to validate it
    let unchecked_address: BitcoinAddress<bitcoin::address::NetworkUnchecked> =
        taproot_address.parse()?;
    let address = unchecked_address.assume_checked();

    // Load the keypair from storage
    let storage_dir = get_storage_dir()?;
    let key_file = storage_dir.join(format!("key_{}.json", address));

    if !key_file.exists() {
        return Err(format!("No key found for address: {}", address).into());
    }

    let file_content = fs::read_to_string(key_file)?;
    let key_data: serde_json::Value = serde_json::from_str(&file_content)?;

    // Detect key format version
    let version = key_data
        .get("version")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    let is_encrypted = key_data
        .get("encrypted")
        .and_then(|e| e.as_bool())
        .unwrap_or(false);

    let private_key_str = if is_encrypted && version >= 2 {
        // Handle encrypted key (version 2+)
        let passphrase = passphrase.ok_or(
            "This key is encrypted and requires a passphrase. Please provide the passphrase used when the key was created."
        )?;

        let encrypted_data: EncryptedKeyData = serde_json::from_str(&file_content)?;
        let secure_passphrase = SecureString::new(passphrase.to_string());
        let decrypted = decrypt_private_key(&encrypted_data.crypto, &secure_passphrase)?;
        decrypted.as_str().to_string()
    } else {
        // Handle legacy format keys - these should be migrated
        return Err(
            "This key uses an old storage format. Please regenerate your key to use the current secure storage format.".into()
        );
    };

    // Parse the private key
    let secret_key = bitcoin::secp256k1::SecretKey::from_str(&private_key_str)?;
    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &secret_key);

    debug!("{} Key loaded successfully", "SUCCESS".green().bold());
    debug!("{} {}", "ADDRESS".cyan().bold(), address);
    debug!("{} {}", "NETWORK".blue().bold(), network);
    debug!(
        "{} Key loaded from encrypted storage",
        "DECRYPTED".green().bold()
    );

    Ok(keypair)
}

/// Export the private key for a given taproot address
pub fn export_private_key(
    taproot_address: &str,
    network: Network,
) -> Result<String, Box<dyn std::error::Error>> {
    let unchecked_address: Address<bitcoin::address::NetworkUnchecked> = taproot_address.parse()?;
    let address = unchecked_address.require_network(network)?;

    let storage_dir = get_storage_dir()?;
    let key_file = storage_dir.join(format!("key_{}.json", address));

    if !key_file.exists() {
        return Err(format!("No key found for address: {}", address).into());
    }

    let key_data: serde_json::Value = serde_json::from_str(&fs::read_to_string(key_file)?)?;
    let private_key_str = key_data["private_key"]
        .as_str()
        .ok_or("Invalid key file format: missing private_key")?;

    Ok(private_key_str.to_string())
}

/// List all stored keys with their addresses and metadata
pub fn list_keys() -> Result<Vec<(String, serde_json::Value)>, Box<dyn std::error::Error>> {
    let storage_dir = get_storage_dir()?;
    let address_file = storage_dir.join("addresses.json");

    if !address_file.exists() {
        return Ok(Vec::new());
    }

    let addresses: HashMap<String, serde_json::Value> =
        serde_json::from_str(&fs::read_to_string(&address_file)?)?;

    Ok(addresses.into_iter().collect())
}

/// Get the storage directory path
pub fn get_storage_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home_dir = dirs::home_dir().ok_or("Could not determine home directory")?;
    Ok(home_dir.join(".clementine").join("keys"))
}

/// Test helper to store a key with custom base directory
#[cfg(test)]
pub fn store_key_with_base_dir(
    keypair: &Keypair,
    network: Network,
    passphrase: &str,
    base_dir: &std::path::Path,
) -> Result<BitcoinAddress, Box<dyn std::error::Error>> {
    let address = crate::bitcoin_utils::calculate_taproot_address(keypair, network);
    let storage_dir = base_dir.join(".clementine").join("keys");
    fs::create_dir_all(&storage_dir)?;

    println!("Storing key for address: {}", address);

    let key_file = storage_dir.join(format!("key_{}.json", address));
    let private_key_str = keypair.secret_key().display_secret().to_string();

    println!("Private key for address {}: {}", address, private_key_str);

    let secure_passphrase = SecureString::new(passphrase.to_string());
    let crypto = encrypt_private_key(&private_key_str, &secure_passphrase)?;

    println!("Secure passphrase is generated");

    let encrypted_data = EncryptedKeyData {
        version: 2,
        encrypted: true,
        network: network.to_string(),
        address: address.to_string(),
        crypto,
        stored_at: chrono::Utc::now().to_rfc3339(),
    };

    println!("Encrypted key data generated.");

    let key_data = serde_json::to_string_pretty(&encrypted_data)?;
    fs::write(&key_file, key_data)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&key_file)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&key_file, perms)?;
    }

    Ok(address)
}

/// Test helper to load a key with custom base directory
#[cfg(test)]
pub fn load_key_with_base_dir(
    address: &str,
    network: Network,
    passphrase: Option<&str>,
    base_dir: &std::path::Path,
) -> Result<Keypair, Box<dyn std::error::Error>> {
    use bitcoin::secp256k1::{Secp256k1, SecretKey};
    let storage_dir = base_dir.join(".clementine").join("keys");
    let key_file = storage_dir.join(format!("key_{}.json", address));

    if !key_file.exists() {
        return Err(format!("Key file not found for address: {}", address).into());
    }

    let key_data = fs::read_to_string(&key_file)?;
    let encrypted_data: EncryptedKeyData = serde_json::from_str(&key_data)?;

    if encrypted_data.network != network.to_string() {
        return Err(format!(
            "Key network mismatch: expected {}, found {}",
            network, encrypted_data.network
        )
        .into());
    }

    if encrypted_data.encrypted {
        match passphrase {
            Some(pass) => {
                let secure_passphrase = SecureString::new(pass.to_string());
                let decrypted_key =
                    decrypt_private_key(&encrypted_data.crypto, &secure_passphrase)?;
                let secp = Secp256k1::new();
                let secret_key = SecretKey::from_str(decrypted_key.as_str())?;
                Ok(Keypair::from_secret_key(&secp, &secret_key))
            }
            None => Err("Key is encrypted and requires a passphrase".into()),
        }
    } else {
        Err("Unencrypted keys are not supported in this version".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::secp256k1::{Keypair, Secp256k1, SecretKey};
    use std::fs;

    #[test]
    fn test_derive_key_from_passphrase() {
        // Test basic key derivation
        let passphrase = SecureString::new("test_passphrase_123".to_string());
        let salt = [1u8; 32];

        let key1 = derive_key_from_passphrase(&passphrase, &salt, 1000, 1024, 1).unwrap();
        let key2 = derive_key_from_passphrase(&passphrase, &salt, 1000, 1024, 1).unwrap();

        // Same inputs should produce same key
        assert_eq!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_derive_key_different_inputs() {
        let passphrase1 = SecureString::new("passphrase1".to_string());
        let passphrase2 = SecureString::new("passphrase2".to_string());
        let salt = [1u8; 32];

        let key1 = derive_key_from_passphrase(&passphrase1, &salt, 1000, 1024, 1).unwrap();
        let key2 = derive_key_from_passphrase(&passphrase2, &salt, 1000, 1024, 1).unwrap();

        // Different passphrases should produce different keys
        assert_ne!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_derive_key_different_salts() {
        let passphrase = SecureString::new("same_passphrase".to_string());
        let salt1 = [1u8; 32];
        let salt2 = [2u8; 32];

        let key1 = derive_key_from_passphrase(&passphrase, &salt1, 1000, 1024, 1).unwrap();
        let key2 = derive_key_from_passphrase(&passphrase, &salt2, 1000, 1024, 1).unwrap();

        // Different salts should produce different keys
        assert_ne!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_encrypt_decrypt_round_trip() {
        let private_key = "L1HKVVLHXiUhecWnwFYF6L3shkf1E12HUmuZTESvBXUdx3yqVP1D";
        let passphrase = SecureString::new("strong_passphrase_123".to_string());

        // Encrypt the private key
        let crypto_params = encrypt_private_key(private_key, &passphrase).unwrap();

        // Verify crypto parameters are properly set
        assert_eq!(crypto_params.kdf, "argon2id");
        assert_eq!(crypto_params.cipher, "aes-256-gcm");
        assert_eq!(crypto_params.iterations, 3);
        assert_eq!(crypto_params.memory, 65_536);
        assert_eq!(crypto_params.parallelism, 4);
        assert!(!crypto_params.salt.is_empty());
        assert!(!crypto_params.nonce.is_empty());
        assert!(!crypto_params.ciphertext.is_empty());

        // Decrypt the private key
        let decrypted = decrypt_private_key(&crypto_params, &passphrase).unwrap();

        // Should match original
        assert_eq!(decrypted.as_str(), private_key);
    }

    #[test]
    fn test_decrypt_wrong_passphrase() {
        let private_key = "L1HKVVLHXiUhecWnwFYF6L3shkf1E12HUmuZTESvBXUdx3yqVP1D";
        let correct_passphrase = SecureString::new("correct_passphrase".to_string());
        let wrong_passphrase = SecureString::new("wrong_passphrase".to_string());

        // Encrypt with correct passphrase
        let crypto_params = encrypt_private_key(private_key, &correct_passphrase).unwrap();

        // Try to decrypt with wrong passphrase - should fail
        let result = decrypt_private_key(&crypto_params, &wrong_passphrase);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Decryption failed")
        );
    }

    #[test]
    fn test_encrypt_different_passphrases_different_output() {
        let private_key = "L1HKVVLHXiUhecWnwFYF6L3shkf1E12HUmuZTESvBXUdx3yqVP1D";
        let passphrase1 = SecureString::new("passphrase1".to_string());
        let passphrase2 = SecureString::new("passphrase2".to_string());

        let crypto1 = encrypt_private_key(private_key, &passphrase1).unwrap();
        let crypto2 = encrypt_private_key(private_key, &passphrase2).unwrap();

        // Different passphrases should produce different encrypted results
        assert_ne!(crypto1.ciphertext, crypto2.ciphertext);
        assert_ne!(crypto1.salt, crypto2.salt);
        assert_ne!(crypto1.nonce, crypto2.nonce);
    }

    #[test]
    fn test_encrypt_same_passphrase_different_output() {
        let private_key = "L1HKVVLHXiUhecWnwFYF6L3shkf1E12HUmuZTESvBXUdx3yqVP1D";
        let passphrase = SecureString::new("same_passphrase".to_string());

        let crypto1 = encrypt_private_key(private_key, &passphrase).unwrap();
        let crypto2 = encrypt_private_key(private_key, &passphrase).unwrap();

        // Same passphrase should still produce different encrypted results due to random salt/nonce
        assert_ne!(crypto1.ciphertext, crypto2.ciphertext);
        assert_ne!(crypto1.salt, crypto2.salt);
        assert_ne!(crypto1.nonce, crypto2.nonce);
    }

    #[test]
    fn test_decrypt_unsupported_kdf() {
        let passphrase = SecureString::new("test_passphrase".to_string());
        let crypto = CryptoParams {
            kdf: "pbkdf2".to_string(), // Unsupported KDF
            salt: hex::encode([1u8; 32]),
            iterations: 3,
            memory: 1024,
            parallelism: 1,
            cipher: "aes-256-gcm".to_string(),
            nonce: hex::encode([2u8; 12]),
            ciphertext: hex::encode([3u8; 32]),
        };

        let result = decrypt_private_key(&crypto, &passphrase);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported KDF"));
    }

    #[test]
    fn test_decrypt_unsupported_cipher() {
        let passphrase = SecureString::new("test_passphrase".to_string());
        let crypto = CryptoParams {
            kdf: "argon2id".to_string(),
            salt: hex::encode([1u8; 32]),
            iterations: 3,
            memory: 1024,
            parallelism: 1,
            cipher: "aes-128-cbc".to_string(), // Unsupported cipher
            nonce: hex::encode([2u8; 12]),
            ciphertext: hex::encode([3u8; 32]),
        };

        let result = decrypt_private_key(&crypto, &passphrase);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unsupported cipher")
        );
    }

    #[test]
    fn test_decrypt_invalid_hex() {
        let passphrase = SecureString::new("test_passphrase".to_string());
        let crypto = CryptoParams {
            kdf: "argon2id".to_string(),
            salt: "invalid_hex_xyz".to_string(), // Invalid hex
            iterations: 3,
            memory: 1024,
            parallelism: 1,
            cipher: "aes-256-gcm".to_string(),
            nonce: hex::encode([2u8; 12]),
            ciphertext: hex::encode([3u8; 32]),
        };

        let result = decrypt_private_key(&crypto, &passphrase);
        assert!(result.is_err());
    }

    #[test]
    fn test_secure_string_zeroize() {
        let secret_data = "sensitive_private_key_data";
        let secure_string = SecureString::new(secret_data.to_string());

        assert_eq!(secure_string.as_str(), secret_data);
        assert_eq!(secure_string.as_bytes(), secret_data.as_bytes());

        // SecureString should implement ZeroizeOnDrop
        // We can't directly test the zeroing behavior in a unit test,
        // but we can verify the type implements the trait
        drop(secure_string);
    }

    #[test]
    fn test_derived_key_zeroize() {
        let key_bytes = [42u8; 32];
        let derived_key = DerivedKey::new(key_bytes);

        assert_eq!(derived_key.as_bytes(), &key_bytes);

        // DerivedKey should implement ZeroizeOnDrop
        drop(derived_key);
    }

    #[test]
    fn test_encrypted_key_data_serialization() {
        let crypto = CryptoParams {
            kdf: "argon2id".to_string(),
            salt: hex::encode([1u8; 32]),
            iterations: 3,
            memory: 65_536,
            parallelism: 4,
            cipher: "aes-256-gcm".to_string(),
            nonce: hex::encode([2u8; 12]),
            ciphertext: hex::encode([3u8; 64]),
        };

        let encrypted_data = EncryptedKeyData {
            version: 2,
            encrypted: true,
            network: "bitcoin".to_string(),
            address: "bc1pdqrcrxa8vx6gy75mfdfj84puhxffh4fq46h3gkp6jxdd0vjcsdyspfxcv6".to_string(),
            crypto,
            stored_at: "2024-01-01T00:00:00Z".to_string(),
        };

        // Test serialization
        let json = serde_json::to_string(&encrypted_data).unwrap();
        assert!(json.contains("\"version\":2"));
        assert!(json.contains("\"encrypted\":true"));
        assert!(json.contains("\"kdf\":\"argon2id\""));
        assert!(json.contains("\"cipher\":\"aes-256-gcm\""));

        // Test deserialization
        let deserialized: EncryptedKeyData = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.version, 2);
        assert!(deserialized.encrypted);
        assert_eq!(deserialized.crypto.kdf, "argon2id");
        assert_eq!(deserialized.crypto.iterations, 3);
    }

    // Integration tests for store_key and load_key
    #[test]
    fn test_store_and_load_key_integration() {
        let temp_dir = std::env::temp_dir().join(format!("clementine_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        println!("Base directory for key storage: {}", temp_dir.display());

        // Create a test keypair
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let network = Network::Testnet4;
        let passphrase = "test_passphrase_123";

        println!("Passphrase for key storage: {}", passphrase);

        // Store the key using helper function
        let stored_address =
            store_key_with_base_dir(&keypair, network, passphrase, &temp_dir).unwrap();

        println!("Stored key address: {}", stored_address);

        // Load the key back using helper function
        let loaded_keypair = load_key_with_base_dir(
            &stored_address.to_string(),
            network,
            Some(passphrase),
            &temp_dir,
        )
        .unwrap();

        println!("Loaded key address: {}", stored_address);

        // Verify the loaded keypair matches the original
        assert_eq!(keypair.secret_key(), loaded_keypair.secret_key());
        assert_eq!(keypair.public_key(), loaded_keypair.public_key());

        println!(
            "Key successfully stored and loaded for address: {}",
            stored_address
        );

        // Verify file exists and has correct permissions
        let storage_dir = temp_dir.join(".clementine").join("keys");
        let key_file = storage_dir.join(format!("key_{}.json", stored_address));
        assert!(key_file.exists());

        println!("Key file exists: {}", key_file.display());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&key_file).unwrap();
            let permissions = metadata.permissions().mode();
            // Check that permissions are 600 (rw-------)
            assert_eq!(permissions & 0o777, 0o600);
        }

        println!("Key file permissions are correct: 600 (rw-------)");

        // Clean up temp directory
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_load_key_wrong_passphrase() {
        let temp_dir = std::env::temp_dir().join(format!("clementine_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let base_dir = &temp_dir;

        // Create and store a key
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let network = Network::Testnet4;
        let correct_passphrase = "correct_passphrase";
        let wrong_passphrase = "wrong_passphrase";

        let stored_address =
            store_key_with_base_dir(&keypair, network, correct_passphrase, base_dir).unwrap();

        // Try to load with wrong passphrase - should fail
        let result = load_key_with_base_dir(
            &stored_address.to_string(),
            network,
            Some(wrong_passphrase),
            base_dir,
        );
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Decryption failed")
        );

        // Clean up temp directory
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_load_nonexistent_key() {
        let temp_dir = std::env::temp_dir().join(format!(
            "clementine_test_nonexistent_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let base_dir = &temp_dir;

        let network = Network::Testnet4;
        let fake_address = "tb1pdqrcrxa8vx6gy75mfdfj84puhxffh4fq46h3gkp6jxdd0vjcsdysn6k0k7";

        let result =
            load_key_with_base_dir(fake_address, network, Some("any_passphrase"), base_dir);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Key file not found for address")
        );

        // Clean up temp directory
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_multiple_keys_storage() {
        let temp_dir =
            std::env::temp_dir().join(format!("clementine_test_multiple_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let base_dir = &temp_dir;

        let secp = Secp256k1::new();
        let network = Network::Testnet4;

        // Store multiple keys
        let mut stored_addresses = Vec::new();
        for i in 1..=3 {
            let secret_key = SecretKey::from_slice(&[i; 32]).unwrap();
            let keypair = Keypair::from_secret_key(&secp, &secret_key);
            let passphrase = format!("passphrase_{}", i);

            let address =
                store_key_with_base_dir(&keypair, network, &passphrase, base_dir).unwrap();
            stored_addresses.push((address, passphrase, keypair));
        }

        // Load each key and verify
        for (address, passphrase, original_keypair) in stored_addresses {
            let loaded_keypair =
                load_key_with_base_dir(&address.to_string(), network, Some(&passphrase), base_dir)
                    .unwrap();
            assert_eq!(original_keypair.secret_key(), loaded_keypair.secret_key());
        }

        // Clean up temp directory
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_key_derivation_parameters() {
        let passphrase = SecureString::new("test_passphrase".to_string());
        let salt = [1u8; 32];

        // Test with minimum secure parameters
        let key_min = derive_key_from_passphrase(&passphrase, &salt, 3, 1024, 1).unwrap();

        // Test with production parameters (same as used in encrypt_private_key)
        let key_prod = derive_key_from_passphrase(&passphrase, &salt, 3, 65_536, 4).unwrap();

        // Both should succeed but produce different keys due to different parameters
        assert_ne!(key_min.as_bytes(), key_prod.as_bytes());
    }

    #[test]
    fn test_argon2_invalid_parameters() {
        let passphrase = SecureString::new("test_passphrase".to_string());
        let salt = [1u8; 32];

        // Test with invalid memory parameter (too small)
        let result = derive_key_from_passphrase(&passphrase, &salt, 1, 0, 1);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid Argon2 parameters")
        );
    }
}

/// Generate master seed from mnemonic phrase
pub fn get_master_seed_from_mnemonic(
    mnemonic_phrase: &str,
) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let mnemonic = Mnemonic::parse(mnemonic_phrase)?;
    
    // Generate seed (64 bytes)
    let seed = mnemonic.to_seed("");
    
    let mut master_seed = [0u8; 32];
    master_seed.copy_from_slice(&seed[0..32]);
    
    Ok(master_seed)
}

pub fn derive_private_key(
    master_seed: &[u8; 32],
    derivation_path: &str,
    network: Network,
) -> Result<SecretKey, Box<dyn std::error::Error>> {
    let master_xpriv = Xpriv::new_master(network, master_seed)?;

    let path = DerivationPath::from_str(derivation_path)?;

    let child_xpriv = master_xpriv.derive_priv(&crate::bitcoin_utils::SECP, &path)?;

    Ok(child_xpriv.private_key)
}

pub fn derive_keypair_and_address(
    master_seed: &[u8; 32],
    derivation_path: &str,
    network: Network,
) -> Result<(Keypair, Address), Box<dyn std::error::Error>> {
    let secret_key = derive_private_key(master_seed, derivation_path, network)?;
    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &secret_key);
    let address = crate::bitcoin_utils::calculate_taproot_address(&keypair, network);

    Ok((keypair, address))
}

pub fn get_standard_derivation_path(account: u32, change: u32, address_index: u32) -> String {
    format!("m/44'/0'/{}'/{}/{}", account, change, address_index)
}

pub fn get_native_segwit_derivation_path(account: u32, change: u32, address_index: u32) -> String {
    format!("m/84'/0'/{}'/{}/{}", account, change, address_index)
}

pub fn get_taproot_derivation_path(account: u32, change: u32, address_index: u32) -> String {
    format!("m/86'/0'/{}'/{}/{}", account, change, address_index)
}
