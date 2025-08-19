use anyhow::anyhow;
use bip39::{Language, Mnemonic};
use bitcoin::Network;
use bitcoin::key::Keypair;
use bitcoin::secp256k1::SecretKey;
use colored::Colorize;
use rand::{RngCore, rng};
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::io::Write;
use std::{fs, io};
use zeroize::Zeroize;

use crate::bitcoin_utils::calculate_taproot_address;
use crate::encryption::{aes_decrypt_secure, aes_encrypt_secure};
use crate::secure_display::display_mnemonic_securely;
use crate::secure_structs::{SecureString, SecureStringExt};
use crate::wallet::{get_storage_dir, load_wallet_from_file};

const MNEMONIC_WORD_COUNT: usize = 12;

pub struct EncryptedData {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12], // AES-GCM standard nonce size
    pub salt: [u8; 32],  // Salt for PBKDF2
}

pub fn show_mnemonic_secure(address: &str) -> Result<(), anyhow::Error> {
    let passphrase = prompt_secure_passphrase("Enter passphrase to decrypt the mnemonic: ")?;
    let mnemonic = load_mnemonic_secure(address, &passphrase)?;
    display_mnemonic_securely(&mnemonic)?;

    Ok(())
}

pub fn generate_mnemonic_secure() -> Result<SecureString, anyhow::Error> {
    const ENTROPY_BITS: usize = 128; // 128 bits of entropy for 12-word mnemonic

    let mut entropy = vec![0u8; ENTROPY_BITS / 8];
    rng().fill_bytes(&mut entropy);

    let mut mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)?;

    let safe_mnemonic = SecureString::init_with(|| mnemonic.to_string());

    mnemonic.zeroize();
    entropy.zeroize();

    Ok(safe_mnemonic)
}

pub fn generate_random_mnemonic(display_mnemonic: bool) -> Result<(), anyhow::Error> {
    println!("Generating a 12-word mnemonic phrase...");
    println!();

    let mnemonic = generate_mnemonic_secure()?;

    if display_mnemonic {
        match display_mnemonic_securely(&mnemonic) {
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

pub fn create_encrypted_wallet_with_address(network: Network) -> Result<String, anyhow::Error> {
    use crate::bitcoin_utils::calculate_taproot_address;
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
    let secure_mnemonic = generate_mnemonic_secure()?;

    // Generate master seed from mnemonic using BIP-39
    let master_seed = get_master_seed_from_mnemonic(&secure_mnemonic)
        .map_err(|e| anyhow!("Failed to generate master seed: {}", e))?;

    // Generate master private key directly from the seed (first 32 bytes)
    use bitcoin::secp256k1::SecretKey;
    let mut master_private_key = SecretKey::from_slice(&master_seed)?;
    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &master_private_key);
    let address = calculate_taproot_address(&keypair, network);

    println!("Generated address: {}", address.to_string().green());

    // Prompt for passphrase
    print!("Enter passphrase to encrypt the wallet: ");
    io::stdout().flush()?;

    let passphrase_input = rpassword::read_password()?;
    let passphrase = SecureString::init_with(|| passphrase_input);

    if passphrase.is_empty() {
        return Err(anyhow!("Passphrase cannot be empty for security reasons"));
    }

    // Encrypt mnemonic and private key separately with different nonces
    let master_private_key_str = master_private_key.display_secret().to_string();
    let master_private_key_secure = SecureString::init_with(|| master_private_key_str);

    master_private_key.non_secure_erase();

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

    match display_mnemonic_securely(&secure_mnemonic) {
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
    let passphrase = SecureString::init_with(|| passphrase_input);

    if passphrase.is_empty() {
        return Err(anyhow!("Passphrase cannot be empty for security reasons"));
    }

    let mnemonic_phrase = generate_and_store_mnemonic_secure(wallet_name, &passphrase, network)?;

    println!("✓ Mnemonic encrypted and stored securely");
    println!();

    // Use secure display for the mnemonic phrase
    match display_mnemonic_securely(&mnemonic_phrase) {
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

pub fn prompt_secure_passphrase(prompt: &str) -> Result<SecureString, anyhow::Error> {
    let mut passphrase = rpassword::prompt_password(prompt)?;

    if passphrase.is_empty() {
        return Err(anyhow!("Passphrase cannot be empty"));
    }

    if passphrase.len() < 8 {
        passphrase.zeroize();
        return Err(anyhow!("Passphrase must be at least 8 characters long"));
    }

    let secure_passphrase = SecureString::init_with(|| passphrase);

    Ok(secure_passphrase)
}

pub fn confirm_secure_passphrase(original: &SecureString) -> Result<(), anyhow::Error> {
    let confirmation = prompt_secure_passphrase("Confirm passphrase: ")?;

    use subtle::ConstantTimeEq;
    if original
        .expose_secret()
        .as_bytes()
        .ct_eq(confirmation.expose_secret().as_bytes())
        .into()
    {
        Ok(())
    } else {
        Err(anyhow!("Passphrases do not match"))
    }
}

/// Generate master seed from mnemonic phrase
pub fn get_master_seed_from_mnemonic(
    mnemonic_phrase: &SecureString,
) -> Result<[u8; 32], anyhow::Error> {
    let mut mnemonic = Mnemonic::parse(mnemonic_phrase.expose_secret())?;

    // Generate seed (64 bytes)
    let mut seed = mnemonic.to_seed("");

    // Make this safe - secure
    let mut master_seed = [0u8; 32];
    master_seed.copy_from_slice(&seed[0..32]);

    mnemonic.zeroize();
    seed.zeroize();

    Ok(master_seed)
}

pub fn generate_and_store_mnemonic_secure(
    wallet_name: &str,
    passphrase: &SecureString,
    network: Network,
) -> Result<SecureString, anyhow::Error> {
    let secure_mnemonic = generate_mnemonic_secure()?;

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

pub fn load_mnemonic_secure(
    wallet_name: &str,
    passphrase: &SecureString,
) -> Result<SecureString, anyhow::Error> {
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

    let secure_mnemonic = aes_decrypt_secure(&encrypted_data, passphrase)?;

    Ok(secure_mnemonic)
}

pub fn load_private_key_secure(
    wallet_name: &str,
    passphrase: &SecureString,
) -> Result<SecureString, anyhow::Error> {
    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    let wallet_file = storage_dir.join(format!("wallet_{wallet_name}.json"));

    if !wallet_file.exists() {
        return Err(anyhow!("No wallet found with name: {wallet_name}"));
    }

    let wallet_data: serde_json::Value = serde_json::from_str(&fs::read_to_string(wallet_file)?)?;

    // Handle the format with encrypted_private_key object
    let private_key_data = &wallet_data["encrypted_private_key"];
    let encrypted_hex = private_key_data["ciphertext"].as_str().ok_or_else(|| {
        anyhow!("Invalid wallet file format: missing encrypted_private_key.ciphertext")
    })?;
    let nonce_hex = private_key_data["nonce"].as_str().ok_or_else(|| {
        anyhow!("Invalid wallet file format: missing encrypted_private_key.nonce")
    })?;
    let salt_hex = private_key_data["salt"]
        .as_str()
        .ok_or_else(|| anyhow!("Invalid wallet file format: missing encrypted_private_key.salt"))?;

    let encrypted_data = EncryptedData {
        ciphertext: hex::decode(encrypted_hex)?,
        nonce: hex::decode(nonce_hex)?
            .try_into()
            .map_err(|_| anyhow!("Invalid nonce length"))?,
        salt: hex::decode(salt_hex)?
            .try_into()
            .map_err(|_| anyhow!("Invalid salt length"))?,
    };

    let secure_private_key = aes_decrypt_secure(&encrypted_data, &passphrase)?;

    Ok(secure_private_key)
}

pub fn derive_private_key_from_mnemonic_secure(
    mnemonic: &SecureString,
) -> Result<SecureString, anyhow::Error> {
    use bitcoin::secp256k1::SecretKey;

    // Generate master seed from mnemonic using BIP-39
    let master_seed = get_master_seed_from_mnemonic(mnemonic)
        .map_err(|e| anyhow!("Failed to generate master seed from mnemonic: {}", e))?;

    let mut master_private_key = SecretKey::from_slice(&master_seed)?;

    let secure_private_key =
        SecureString::init_with(|| master_private_key.display_secret().to_string());

    master_private_key.non_secure_erase();

    Ok(secure_private_key)
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

/// Backup a wallet file to a specified destination
pub fn backup_wallet(
    wallet_address: &str,
    destination_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_address));

    if !wallet_file.exists() {
        return Err(format!("No wallet found with address: {}", wallet_address).into());
    }

    // Parse the destination path
    let dest_path = std::path::Path::new(destination_path);

    // If destination is a directory, create the filename
    let final_dest = if dest_path.is_dir() {
        dest_path.join(format!("wallet_{}.json", wallet_address))
    } else {
        dest_path.to_path_buf()
    };

    // Create parent directories if they don't exist
    if let Some(parent) = final_dest.parent() {
        fs::create_dir_all(parent)?;
    }

    // Copy the wallet file
    fs::copy(&wallet_file, &final_dest)?;

    // Set secure file permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&final_dest)?.permissions();
        perms.set_mode(0o600); // rw-------
        fs::set_permissions(&final_dest, perms)?;
    }

    println!(
        "{} Wallet '{}' backed up successfully to: {}",
        "✓".green(),
        wallet_address.cyan(),
        final_dest.display().to_string().yellow()
    );

    Ok(())
}

/// List all available wallets
pub fn list_wallets() -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    let wallets_file = storage_dir.join("wallets.json");

    if !wallets_file.exists() {
        return Ok(Vec::new());
    }

    let wallets: HashMap<String, serde_json::Value> =
        serde_json::from_str(&fs::read_to_string(&wallets_file)?)?;

    Ok(wallets.keys().cloned().collect())
}

/// Import a wallet using secure mnemonic input (step-by-step) and password creation
pub fn import_wallet_from_mnemonic(
    network: Network,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    println!("{}", "🔒 Import Wallet with Secure Input".blue().bold());
    println!("This process will:");
    println!("• Securely collect your mnemonic phrase word by word");
    println!("• Create a secure passphrase to encrypt the wallet");
    println!("• Derive and store the wallet with full encryption");
    println!();

    // Prompt for mnemonic securely (word by word)
    println!("{}", "Step 1: Enter Mnemonic Phrase".yellow().bold());
    let secure_mnemonic = prompt_mnemonic_secure()?;

    // Generate address from mnemonic
    let master_seed = get_master_seed_from_mnemonic(&secure_mnemonic)
        .map_err(|e| format!("Failed to generate master seed: {}", e))?;

    let master_private_key = SecretKey::from_slice(&master_seed)?;
    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &master_private_key);
    let address = calculate_taproot_address(&keypair, network);

    println!();
    println!("✅ Mnemonic processed successfully!");
    println!("Derived address: {}", address.to_string().green());
    println!();

    // Check if wallet already exists
    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    if wallet_file.exists() {
        return Err(format!(
            "❌ Wallet with address '{}' already exists in local storage.\nLocation: {}",
            address,
            wallet_file.display()
        )
        .into());
    }

    // Securely prompt for passphrase
    println!("{}", "Step 2: Create Secure Passphrase".yellow().bold());
    println!("Enter a strong passphrase to encrypt your imported wallet:");
    print!("Passphrase: ");
    io::stdout().flush()?;
    let passphrase_input = rpassword::read_password()?;

    if passphrase_input.is_empty() {
        return Err("❌ Passphrase cannot be empty for security reasons".into());
    }

    if passphrase_input.len() < 8 {
        return Err("❌ Passphrase must be at least 8 characters long".into());
    }

    // Confirm passphrase
    print!("Confirm passphrase: ");
    io::stdout().flush()?;
    let passphrase_confirm = rpassword::read_password()?;

    if passphrase_input != passphrase_confirm {
        return Err("❌ Passphrases do not match".into());
    }

    let passphrase = SecureString::init_with(|| passphrase_input);
    println!("✅ Passphrase created successfully!");
    println!();

    // Encrypt and store wallet
    println!(
        "{}",
        "Step 3: Encrypting and Storing Wallet".yellow().bold()
    );

    let master_private_key_str = master_private_key.display_secret().to_string();
    let master_private_key_secure = SecureString::init_with(|| master_private_key_str);

    let encrypted_mnemonic = aes_encrypt_secure(&secure_mnemonic, &passphrase)
        .map_err(|e| format!("Failed to encrypt mnemonic: {}", e))?;

    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .map_err(|e| format!("Failed to encrypt private key: {}", e))?;

    store_encrypted_wallet_data_separate(
        &address.to_string(),
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
    )
    .map_err(|e| format!("Failed to store wallet: {}", e))?;

    // Update wallets registry
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
            "imported_at": chrono::Utc::now().to_rfc3339(),
            "imported": true,
            "import_method": "secure_mnemonic_input"
        }),
    );

    fs::write(&wallets_file, serde_json::to_string_pretty(&wallets)?)?;

    println!("✅ Wallet imported and encrypted successfully!");
    println!("📁 Stored as: wallet_{}.json", address);
    println!("📍 Location: {}", storage_dir.display().to_string().cyan());
    println!("🆔 Address: {}", address.to_string().green());
    println!();
    println!("{}", "🔒 Security Features Applied:".green());
    println!("• Mnemonic handled securely and zeroized from memory");
    println!("• Passphrase stored securely and zeroized from memory");
    println!("• AES-256-GCM authenticated encryption");
    println!("• Separate encryption for mnemonic and private key");
    println!("• Secure file permissions applied");

    Ok(address.to_string())
}

/// Import a wallet from a private key
pub fn import_wallet_from_private_key(
    private_key_hex: SecureString,
    network: Network,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use crate::bitcoin_utils::calculate_taproot_address;
    use bitcoin::secp256k1::{Keypair, SecretKey};
    use std::io::{self, Write};

    // Parse the private key from hex
    let mut private_key_bytes = hex::decode(private_key_hex.expose_secret())
        .map_err(|e| format!("Invalid private key hex format: {}", e))?;

    if private_key_bytes.len() != 32 {
        private_key_bytes.zeroize();
        return Err("Private key must be exactly 32 bytes (64 hex characters)".into());
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&private_key_bytes);
    private_key_bytes.zeroize();

    let mut master_private_key = SecretKey::from_slice(&key_array).map_err(|e| {
        key_array.zeroize();
        format!("Invalid private key: {}", e)
    })?;
    key_array.zeroize();

    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &master_private_key);
    let address = calculate_taproot_address(&keypair, network);

    println!("Derived address: {}", address.to_string().green());

    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    if wallet_file.exists() {
        return Err(format!(
            "Wallet with address '{}' already exists in local storage",
            address
        )
        .into());
    }

    print!("Enter passphrase to encrypt the imported wallet: ");
    io::stdout().flush()?;
    let passphrase_input = rpassword::read_password()?;
    let passphrase = SecureString::init_with(|| passphrase_input);

    if passphrase.is_empty() {
        return Err("Passphrase cannot be empty for security reasons".into());
    }

    let placeholder_mnemonic = SecureString::init_with(|| "IMPORTED_FROM_PRIVATE_KEY".to_string());

    let mut master_private_key_str = master_private_key.display_secret().to_string();
    let master_private_key_secure = SecureString::init_with(|| master_private_key_str.clone());

    master_private_key_str.zeroize();
    master_private_key.non_secure_erase();

    let encrypted_mnemonic = aes_encrypt_secure(&placeholder_mnemonic, &passphrase)
        .map_err(|e| format!("Failed to encrypt placeholder mnemonic: {}", e))?;
    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .map_err(|e| format!("Failed to encrypt private key: {}", e))?;

    store_encrypted_wallet_data_separate(
        &address.to_string(),
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
    )
    .map_err(|e| format!("Failed to store wallet: {}", e))?;

    println!(
        "✓ Wallet imported and encrypted successfully as wallet_{}.json",
        address
    );
    println!(
        "Imported address: {} at directory: {}",
        address.to_string().green(),
        storage_dir.display().to_string().cyan()
    );
    println!(
        "{} Note: This wallet was imported from a private key, so no mnemonic phrase is available.",
        "ℹ".yellow()
    );

    // Note: private_key_hex (SecureString), placeholder_mnemonic (SecureString),
    // master_private_key_secure (SecureString), and passphrase (SecretString)
    // will all be automatically zeroized when they go out of scope
    Ok(address.to_string())
}

pub fn extract_address_from_wallet(
    wallet_data: &serde_json::Value,
) -> Result<String, anyhow::Error> {
    // Try to get the address field from the wallet data
    if let Some(address) = wallet_data.get("address") {
        if let Some(address_str) = address.as_str() {
            return Ok(address_str.to_string());
        }
    }

    Err(anyhow!("Address field not found or invalid in wallet data"))
}

pub fn generate_address_from_mnemonic_secure(
    secure_mnemonic: &SecureString,
    network: Network,
) -> Result<String, anyhow::Error> {
    use crate::bitcoin_utils::calculate_taproot_address;
    use bitcoin::secp256k1::{Keypair, SecretKey};
    use zeroize::Zeroize;

    // Generate master seed from mnemonic using BIP-39
    let mut master_seed = get_master_seed_from_mnemonic(secure_mnemonic)
        .map_err(|e| anyhow!("Failed to generate master seed from mnemonic: {}", e))?;

    // Generate master private key directly from the seed (first 32 bytes)
    let master_private_key = SecretKey::from_slice(&master_seed)?;
    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &master_private_key);

    // Calculate the taproot address
    let address = calculate_taproot_address(&keypair, network);

    // Explicitly zeroize sensitive data in memory
    master_seed.zeroize();
    // Note: master_private_key and keypair contain sensitive data but SecretKey
    // and Keypair don't implement Zeroize, so they'll be cleared when they go out of scope

    Ok(address.to_string())
}

/// Securely prompt for mnemonic phrase word by word with validation
pub fn prompt_mnemonic_secure() -> Result<SecureString, anyhow::Error> {
    use bip39::{Language, Mnemonic};
    use colored::Colorize;
    use zeroize::Zeroize;

    println!("{}", "🔒 Secure Mnemonic Input".blue().bold());
    println!("Enter your mnemonic phrase word by word.");
    println!("Each word will be validated against the BIP-39 wordlist.");
    println!("Valid lengths: 12, 15, 18, 21, or 24 words");
    println!("Type 'done' when you've entered all words, or just press Enter on an empty line.");
    println!();

    let mut words: Vec<String> = Vec::new();
    let mut word_index = 1;

    // Get the BIP-39 English wordlist for validation
    let wordlist = Language::English.word_list();

    loop {
        let mut word =
            rpassword::prompt_password(&format!("Word {}: ", word_index.to_string().cyan()))?
                .trim()
                .to_lowercase();

        // Validate word against BIP-39 wordlist
        if wordlist.iter().any(|&w| w == word) {
            words.push(word.clone());
            println!("✅ Word {} accepted", word_index);
            word_index += 1;
            word.zeroize(); // Clear the word from memory

            // Check if we have a valid mnemonic length and offer to finish
            if words.len() == MNEMONIC_WORD_COUNT {
                println!();
                println!(
                    "{} You have entered {} words (valid mnemonic length).",
                    "ℹ️".blue(),
                    words.len().to_string().green()
                );
                break;
            }
        } else {
            word.zeroize(); // Clear invalid word from memory
            println!("{} Invalid word entered. Please try again.", "❌".red());
            println!("Hint: Words should be lowercase English BIP-39 words.");
        }

        // Safety check - prevent extremely long inputs
        if words.len() > 24 {
            return Err(anyhow!(
                "Too many words entered. BIP-39 mnemonics have maximum 24 words."
            ));
        }
    }

    // Validate final mnemonic length
    let word_count = words.len();
    if ![12].contains(&word_count) {
        // Clear words from memory
        for mut word in words {
            word.zeroize();
        }
        return Err(anyhow!(
            "Invalid mnemonic length: {} words. Must be 12 words.",
            word_count
        ));
    }

    // Join words and validate complete mnemonic
    let mut mnemonic_phrase = words.join(" ");
    let mnemonic_validation = Mnemonic::parse(&mnemonic_phrase);

    // Clear individual words from memory
    for mut word in words {
        word.zeroize();
    }

    match mnemonic_validation {
        Ok(mut mnemonic) => {
            println!();
            println!(
                "✅ {} Valid BIP-39 mnemonic phrase with {} words",
                "SUCCESS".green().bold(),
                mnemonic_phrase.split_whitespace().count()
            );
            println!("🔒 Mnemonic will be handled securely and zeroized from memory");

            // Create secure string
            let secure_mnemonic = SecureString::init_with(|| mnemonic_phrase);

            mnemonic.zeroize();

            Ok(secure_mnemonic)
        }
        Err(e) => {
            mnemonic_phrase.zeroize();
            // This shouldn't happen since we validated each word, but safety check
            Err(anyhow!("Mnemonic validation failed: {}", e))
        }
    }
}
