use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use anyhow::anyhow;
use bip39::{Language, Mnemonic};
use bitcoin::Network;
use colored::Colorize;
use rand::{RngCore, rng};
use secrecy::{ExposeSecret, SecretString};
use std::collections::HashMap;
use std::fs;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::storage::get_storage_dir;

/// Secure wrapper for sensitive strings that auto-zeroizes on drop
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecureString(String);

impl SecureString {
    pub fn new(s: String) -> Self {
        SecureString(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

pub type SecurePassphrase = SecretString;

pub trait SecurePassphraseExt {
    fn from_str(s: String) -> Self;
    fn is_empty(&self) -> bool;
    fn as_bytes(&self) -> &[u8];
}

impl SecurePassphraseExt for SecretString {
    fn from_str(s: String) -> Self {
        SecretString::new(s.into_boxed_str())
    }

    fn is_empty(&self) -> bool {
        self.expose_secret().is_empty()
    }

    fn as_bytes(&self) -> &[u8] {
        self.expose_secret().as_bytes()
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecureKey([u8; 32]);

impl SecureKey {
    pub fn new(key: [u8; 32]) -> Self {
        SecureKey(key)
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

struct EncryptedData {
    ciphertext: Vec<u8>,
    nonce: [u8; 12], // AES-GCM standard nonce size
    salt: [u8; 32],  // Salt for PBKDF2
}

pub fn generate_mnemonic_secure(word_count: usize) -> Result<SecureString, anyhow::Error> {
    let entropy_bits = match word_count {
        12 => 128,
        15 => 160,
        18 => 192,
        21 => 224,
        24 => 256,
        _ => return Err(anyhow!("Invalid word count. Use 12, 15, 18, 21, or 24")),
    };

    let mut entropy = vec![0u8; entropy_bits / 8];
    rng().fill_bytes(&mut entropy);

    let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)?;

    entropy.zeroize();

    Ok(SecureString::new(mnemonic.to_string()))
}

pub fn create_encrypted_wallet(
    wallet_name: &str,
    word_count: usize,
    network: Network,
) -> Result<SecureString, anyhow::Error> {
    use std::io::{self, Write};

    println!(
        "Creating wallet '{}' with maximum security protection",
        wallet_name.green()
    );
    println!();
    println!("{}", "Security Features:".yellow());
    println!("• Secure terminal input (no echo)");
    println!("• Industry-standard secret handling (secrecy crate)");
    println!("• Memory zeroization");
    println!("• AES-256-GCM authenticated encryption");
    println!("• PBKDF2 key derivation (100,000 iterations)");
    println!();

    print!("Enter passphrase to encrypt the mnemonic: ");
    io::stdout().flush()?;
    let passphrase_input = rpassword::read_password()?;
    let passphrase = SecurePassphrase::from_str(passphrase_input);

    if passphrase.is_empty() {
        return Err(anyhow!("Passphrase cannot be empty for security reasons"));
    }

    let mnemonic_phrase =
        generate_and_store_mnemonic_secure(word_count, wallet_name, &passphrase, network)?;

    println!("✓ Mnemonic encrypted and stored securely");
    println!();
    println!("{}", "IMPORTANT SECURITY NOTICE:".red());
    println!("Please write down your mnemonic phrase and store it in a safe place:");
    println!();
    println!("{}", mnemonic_phrase.as_str().bright_yellow());
    println!();
    println!("This is the ONLY way to recover your wallet if you lose your passphrase!");
    println!("The mnemonic will be cleared from memory after this display.");

    Ok(mnemonic_phrase)
}

pub fn prompt_secure_passphrase(prompt: &str) -> Result<SecurePassphrase, anyhow::Error> {
    let mut passphrase = rpassword::prompt_password(prompt)?;

    if passphrase.is_empty() {
        return Err(anyhow!("Passphrase cannot be empty"));
    }

    if passphrase.len() < 8 {
        passphrase.zeroize();
        return Err(anyhow!("Passphrase must be at least 8 characters long"));
    }

    let secure_passphrase = SecurePassphrase::new(passphrase.clone().into_boxed_str());
    passphrase.zeroize();

    Ok(secure_passphrase)
}

pub fn confirm_secure_passphrase(original: &SecurePassphrase) -> Result<(), anyhow::Error> {
    let confirmation = prompt_secure_passphrase("Confirm passphrase: ")?;

    use subtle::ConstantTimeEq;
    if original.as_bytes().ct_eq(confirmation.as_bytes()).into() {
        Ok(())
    } else {
        Err(anyhow!("Passphrases do not match"))
    }
}

fn derive_key_pbkdf2_secure(
    secure_passphrase: &SecurePassphrase,
    salt: &[u8],
) -> Result<SecureKey, anyhow::Error> {
    use pbkdf2::pbkdf2_hmac;
    use sha2::Sha256;

    println!("Deriving key using PBKDF2 with 100,000 iterations...");
    const ITERATIONS: u32 = 100_000; // OWASP recommended minimum
    let mut key = [0u8; 32];

    pbkdf2_hmac::<Sha256>(secure_passphrase.as_bytes(), salt, ITERATIONS, &mut key);

    let secure_key = SecureKey::new(key);

    key.zeroize();

    Ok(secure_key)
}

fn aes_encrypt_secure(
    secure_plaintext: &SecureString,
    secure_passphrase: &SecurePassphrase,
) -> Result<EncryptedData, anyhow::Error> {
    let mut salt = [0u8; 32];
    let mut nonce_bytes = [0u8; 12];
    rng().fill_bytes(&mut salt);
    rng().fill_bytes(&mut nonce_bytes);

    // Use secure key wrapper that auto-zeroizes
    let secure_key = derive_key_pbkdf2_secure(secure_passphrase, &salt)?;

    let cipher = Aes256Gcm::new(GenericArray::from_slice(secure_key.as_slice()));
    let nonce = GenericArray::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, secure_plaintext.as_bytes())
        .map_err(|e| anyhow!("Encryption failed: {}", e))?;

    Ok(EncryptedData {
        ciphertext,
        nonce: nonce_bytes,
        salt,
    })
}

fn aes_decrypt_secure(
    encrypted_data: &EncryptedData,
    secure_passphrase: &SecurePassphrase,
) -> Result<SecureString, anyhow::Error> {
    let secure_key = derive_key_pbkdf2_secure(secure_passphrase, &encrypted_data.salt)?;

    let cipher = Aes256Gcm::new(GenericArray::from_slice(secure_key.as_slice()));
    let nonce = GenericArray::from_slice(&encrypted_data.nonce);

    let plaintext = cipher
        .decrypt(nonce, encrypted_data.ciphertext.as_ref())
        .map_err(|e| anyhow!("Decryption failed: {}", e))?;

    let plaintext_string = String::from_utf8(plaintext)?;
    Ok(SecureString::new(plaintext_string))
}

pub fn generate_and_store_mnemonic_secure(
    word_count: usize,
    wallet_name: &str,
    passphrase: &SecurePassphrase,
    network: Network,
) -> Result<SecureString, anyhow::Error> {
    let secure_mnemonic = generate_mnemonic_secure(word_count)?;

    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    fs::create_dir_all(&storage_dir)?;

    let wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_name));
    if wallet_file.exists() {
        return Err(anyhow!(
            "Wallet '{}' already exists. Choose a different name or use a different function to overwrite.",
            wallet_name
        ));
    }

    let encrypted_data = aes_encrypt_secure(&secure_mnemonic, &passphrase)?;

    let wallet_data = serde_json::json!({
        "wallet_name": wallet_name,
        "network": network.to_string(),
        "encrypted_data": hex::encode(&encrypted_data.ciphertext),
        "nonce": hex::encode(&encrypted_data.nonce),
        "salt": hex::encode(&encrypted_data.salt),
        "word_count": word_count,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "encryption_method": "aes256_gcm_pbkdf2_secure"
    });

    fs::write(&wallet_file, serde_json::to_string_pretty(&wallet_data)?)?;

    let wallets_file = storage_dir.join("wallets.json");
    let mut wallets: HashMap<String, serde_json::Value> = if wallets_file.exists() {
        serde_json::from_str(&fs::read_to_string(&wallets_file)?)?
    } else {
        HashMap::new()
    };

    wallets.insert(
        wallet_name.to_string(),
        serde_json::json!({
            "network": network.to_string(),
            "word_count": word_count,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "secure": true
        }),
    );

    fs::write(wallets_file, serde_json::to_string_pretty(&wallets)?)?;

    Ok(secure_mnemonic)
}

pub fn load_mnemonic_secure(
    wallet_name: &str,
    passphrase: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Wrap passphrase in secure wrapper and immediately clear the input
    
    // TODO: Read the passphrase from the user in that method remove param 
    let passphrase_copy = passphrase.to_string();
    let secure_passphrase = SecurePassphrase::from_str(passphrase_copy);

    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_name));

    if !wallet_file.exists() {
        return Err(format!("No wallet found with name: {}", wallet_name).into());
    }

    let wallet_data: serde_json::Value = serde_json::from_str(&fs::read_to_string(wallet_file)?)?;
    let encrypted_hex = wallet_data["encrypted_data"]
        .as_str()
        .ok_or("Invalid wallet file format: missing encrypted_data")?;
    let nonce_hex = wallet_data["nonce"]
        .as_str()
        .ok_or("Invalid wallet file format: missing nonce")?;
    let salt_hex = wallet_data["salt"]
        .as_str()
        .ok_or("Invalid wallet file format: missing salt")?;

    let encrypted_data = EncryptedData {
        ciphertext: hex::decode(encrypted_hex)?,
        nonce: hex::decode(nonce_hex)?
            .try_into()
            .map_err(|_| "Invalid nonce length")?,
        salt: hex::decode(salt_hex)?
            .try_into()
            .map_err(|_| "Invalid salt length")?,
    };

    let secure_mnemonic = aes_decrypt_secure(&encrypted_data, &secure_passphrase)?;

    let mnemonic_copy = secure_mnemonic.as_str().to_string();

    Ok(mnemonic_copy)
}
