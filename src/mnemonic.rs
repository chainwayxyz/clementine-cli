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

use crate::secure_display::display_mnemonic_securely;
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

pub struct EncryptedData {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12], // AES-GCM standard nonce size
    pub salt: [u8; 32],  // Salt for PBKDF2
}

pub fn show_mnemonic_secure(address: &str) -> Result<(), anyhow::Error> {
    let passphrase = prompt_secure_passphrase("Enter passphrase to decrypt the mnemonic: ")?;
    let mnemonic = load_mnemonic_secure(address, passphrase.expose_secret())?;
    display_mnemonic_securely(SecureString::new(mnemonic))?;
    Ok(())
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

pub fn generate_random_mnemonic(
    word_count: usize,
    display_mnemonic: bool,
) -> Result<(), anyhow::Error> {
    println!("Generating a {word_count}-word mnemonic phrase...");
    println!();

    let mnemonic = generate_mnemonic_secure(word_count)?;

    if display_mnemonic {
        match display_mnemonic_securely(mnemonic) {
            Ok(()) => {
                println!("✅ Mnemonic generated and displayed securely");
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to display mnemonic securely: {}",
                    e
                ));
            }
        }
    } else {
        println!("✅ Mnemonic generated (not displayed)");
    }

    Ok(())
}

pub fn create_encrypted_wallet_with_address(
    word_count: usize,
    network: Network,
) -> Result<String, anyhow::Error> {
    use crate::{
        bitcoin_utils::calculate_taproot_address,
        storage::{get_master_seed_from_mnemonic, get_storage_dir},
    };
    use bitcoin::secp256k1::Keypair;
    use std::io::{self, Write};

    println!("Creating new wallet with maximum security protection");
    println!();
    println!("{}", "Security Features:".yellow());
    println!("• Secure terminal input (no echo)");
    println!("• Industry-standard secret handling (secrecy crate)");
    println!("• Memory zeroization");
    println!("• AES-256-GCM authenticated encryption");
    println!("• PBKDF2 key derivation (100,000 iterations)");
    println!();

    // Generate mnemonic
    let secure_mnemonic = generate_mnemonic_secure(word_count)?;

    // Generate master seed from mnemonic using BIP-39
    let master_seed = get_master_seed_from_mnemonic(secure_mnemonic.as_str())
        .map_err(|e| anyhow!("Failed to generate master seed: {}", e))?;

    // Generate master private key directly from the seed (first 32 bytes)
    use bitcoin::secp256k1::SecretKey;
    let master_private_key = SecretKey::from_slice(&master_seed)?;
    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &master_private_key);
    let address = calculate_taproot_address(&keypair, network);

    println!("Generated address: {}", address.to_string().green());

    // Prompt for passphrase
    print!("Enter passphrase to encrypt the wallet: ");
    io::stdout().flush()?;
    let passphrase_input = rpassword::read_password()?;
    let passphrase = SecurePassphrase::from_str(passphrase_input);

    if passphrase.is_empty() {
        return Err(anyhow!("Passphrase cannot be empty for security reasons"));
    }

    // Encrypt mnemonic and private key separately with different nonces
    let master_private_key_str = master_private_key.display_secret().to_string();
    let master_private_key_secure = SecureString::new(master_private_key_str);

    let encrypted_mnemonic = aes_encrypt_secure(&secure_mnemonic, &passphrase)?;
    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)?;

    // Store encrypted wallet with separate encrypted fields
    store_encrypted_wallet_data_separate(
        &address.to_string(),
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
    )?;

    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    println!(
        "✓ Wallet encrypted and stored securely as wallet_{}.json",
        address
    );
    println!(
        "Generated address: {} at directory: {}",
        address.to_string().green(),
        storage_dir.display().to_string().cyan()
    );
    println!();

    // Display mnemonic securely
    let mnemonic_copy = SecureString::new(secure_mnemonic.as_str().to_string());
    match display_mnemonic_securely(mnemonic_copy) {
        Ok(()) => {
            println!("{}", "✓ Mnemonic displayed securely".green());
        }
        Err(e) => {
            eprintln!(
                "{} Failed to display mnemonic securely: {}",
                "ERROR".red().bold(),
                e
            );
            eprintln!(
                "{} The wallet is still safely stored encrypted.",
                "INFO".blue().bold()
            );
        }
    }

    Ok(address.to_string())
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

    // Use secure display for the mnemonic phrase
    let mnemonic_copy = SecureString::new(mnemonic_phrase.as_str().to_string());
    match display_mnemonic_securely(mnemonic_copy) {
        Ok(()) => {
            println!("{}", "✓ Mnemonic displayed securely".green());
        }
        Err(e) => {
            eprintln!(
                "{} Failed to display mnemonic securely: {}",
                "ERROR".red().bold(),
                e
            );
            eprintln!(
                "{} The mnemonic is still safely stored encrypted.",
                "INFO".blue().bold()
            );
        }
    }

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

pub fn aes_encrypt_secure(
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

pub fn aes_decrypt_secure(
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

    let wallet_file = storage_dir.join(format!("wallet_{wallet_name}.json"));
    if wallet_file.exists() {
        return Err(anyhow!(
            "Wallet '{}' already exists. Choose a different name or use a different function to overwrite.",
            wallet_name
        ));
    }

    let encrypted_data = aes_encrypt_secure(&secure_mnemonic, passphrase)?;

    let wallet_data = serde_json::json!({
        "wallet_name": wallet_name,
        "network": network.to_string(),
        "encrypted_data": hex::encode(encrypted_data.ciphertext),
        "nonce": hex::encode(encrypted_data.nonce),
        "salt": hex::encode(encrypted_data.salt),
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
                "created_at": chrono::Utc::now().to_rfc3339(),
            "secure": true
        }),
    );

    fs::write(wallets_file, serde_json::to_string_pretty(&wallets)?)?;

    Ok(secure_mnemonic)
}

pub fn load_mnemonic_secure(wallet_name: &str, passphrase: &str) -> Result<String, anyhow::Error> {
    // Wrap passphrase in secure wrapper and immediately clear the input

    // TODO: Read the passphrase from the user in that method remove param
    let passphrase_copy = passphrase.to_string();
    let secure_passphrase = SecurePassphrase::from_str(passphrase_copy);

    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    let wallet_file = storage_dir.join(format!("wallet_{wallet_name}.json"));

    if !wallet_file.exists() {
        return Err(anyhow!("No wallet found with name: {wallet_name}"));
    }

    let wallet_data: serde_json::Value = serde_json::from_str(&fs::read_to_string(wallet_file)?)?;

    // Handle the new format with nested encrypted_mnemonic object
    let mnemonic_data = &wallet_data["encrypted_mnemonic"];
    let encrypted_hex = mnemonic_data["ciphertext"].as_str().ok_or_else(|| {
        anyhow!("Invalid wallet file format: missing encrypted_mnemonic.ciphertext")
    })?;
    let nonce_hex = mnemonic_data["nonce"]
        .as_str()
        .ok_or_else(|| anyhow!("Invalid wallet file format: missing encrypted_mnemonic.nonce"))?;
    let salt_hex = mnemonic_data["salt"]
        .as_str()
        .ok_or_else(|| anyhow!("Invalid wallet file format: missing encrypted_mnemonic.salt"))?;

    let encrypted_data = EncryptedData {
        ciphertext: hex::decode(encrypted_hex)?,
        nonce: hex::decode(nonce_hex)?
            .try_into()
            .map_err(|_| anyhow!("Invalid nonce length"))?,
        salt: hex::decode(salt_hex)?
            .try_into()
            .map_err(|_| anyhow!("Invalid salt length"))?,
    };

    let secure_mnemonic = aes_decrypt_secure(&encrypted_data, &secure_passphrase)?;

    let mnemonic_copy = secure_mnemonic.as_str().to_string();

    Ok(mnemonic_copy)
}

pub fn store_encrypted_wallet_data_separate(
    address: &str,
    network: Network,
    encrypted_mnemonic: &EncryptedData,
    encrypted_private_key: &EncryptedData,
) -> Result<(), anyhow::Error> {
    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    fs::create_dir_all(&storage_dir)?;

    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));
    if wallet_file.exists() {
        return Err(anyhow!(
            "Wallet with address '{}' already exists. Choose a different address or use a different function to overwrite.",
            address
        ));
    }

    let wallet_data_json = serde_json::json!({
        "address": address,
        "network": network.to_string(),
        "encrypted_mnemonic": {
            "ciphertext": hex::encode(&encrypted_mnemonic.ciphertext),
            "nonce": hex::encode(&encrypted_mnemonic.nonce),
            "salt": hex::encode(&encrypted_mnemonic.salt)
        },
        "encrypted_private_key": {
            "ciphertext": hex::encode(&encrypted_private_key.ciphertext),
            "nonce": hex::encode(&encrypted_private_key.nonce),
            "salt": hex::encode(&encrypted_private_key.salt)
        },
        "created_at": chrono::Utc::now().to_rfc3339(),
        "encryption_method": "aes256_gcm_pbkdf2_secure",
        "data_format": "separate_encrypted_fields"
    });

    fs::write(
        &wallet_file,
        serde_json::to_string_pretty(&wallet_data_json)?,
    )?;

    let wallets_file = storage_dir.join("wallets.json");
    let mut wallets: HashMap<String, serde_json::Value> = if wallets_file.exists() {
        serde_json::from_str(&fs::read_to_string(&wallets_file)?)?
    } else {
        HashMap::new()
    };

    wallets.insert(
        address.to_string(),
        serde_json::json!({
            "network": network.to_string(),
                "created_at": chrono::Utc::now().to_rfc3339(),
            "secure": true,
            "data_format": "separate_encrypted_fields"
        }),
    );

    fs::write(wallets_file, serde_json::to_string_pretty(&wallets)?)?;

    Ok(())
}

pub fn store_encrypted_wallet_data(
    address: &str,
    network: Network,
    encrypted_data: &EncryptedData,
) -> Result<(), anyhow::Error> {
    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    fs::create_dir_all(&storage_dir)?;

    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));
    if wallet_file.exists() {
        return Err(anyhow!(
            "Wallet with address '{}' already exists. Choose a different address or use a different function to overwrite.",
            address
        ));
    }

    let wallet_data_json = serde_json::json!({
        "address": address,
        "network": network.to_string(),
        "encrypted_data": hex::encode(&encrypted_data.ciphertext),
        "nonce": hex::encode(&encrypted_data.nonce),
        "salt": hex::encode(&encrypted_data.salt),
        "created_at": chrono::Utc::now().to_rfc3339(),
        "encryption_method": "aes256_gcm_pbkdf2_secure",
        "data_format": "mnemonic|master_private_key"
    });

    fs::write(
        &wallet_file,
        serde_json::to_string_pretty(&wallet_data_json)?,
    )?;

    let wallets_file = storage_dir.join("wallets.json");
    let mut wallets: HashMap<String, serde_json::Value> = if wallets_file.exists() {
        serde_json::from_str(&fs::read_to_string(&wallets_file)?)?
    } else {
        HashMap::new()
    };

    wallets.insert(
        address.to_string(),
        serde_json::json!({
            "network": network.to_string(),
                "created_at": chrono::Utc::now().to_rfc3339(),
            "secure": true,
            "data_format": "mnemonic|master_private_key"
        }),
    );

    fs::write(wallets_file, serde_json::to_string_pretty(&wallets)?)?;

    Ok(())
}
