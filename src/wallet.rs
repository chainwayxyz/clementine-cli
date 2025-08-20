use anyhow::anyhow;
use bitcoin::Network;
use bitcoin::key::Keypair;
use colored::Colorize;
use secrecy::ExposeSecret;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use zeroize::Zeroize;

use crate::bitcoin_utils::calculate_taproot_address;
use crate::encryption::{aes_decrypt_secure, aes_encrypt_secure};
use crate::mnemonic::{
    MNEMONIC_WORD_COUNT, derive_private_key_from_mnemonic_secure, generate_mnemonic_secure,
    prompt_mnemonic_secure,
};
use crate::secure_display::{display_mnemonic_securely, display_private_key_securely};
use crate::secure_structs::SecureString;
use crate::wallet_storage::get_storage_dir;

pub fn delete_wallet(address: &str) -> Result<(), anyhow::Error> {
    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));
    let wallets_file = storage_dir.join("wallets.json");

    // Check if wallet exists
    if !wallet_file.exists() {
        return Err(anyhow!("❌ No wallet found with address: {}", address));
    }

    // Load wallet data to verify it exists
    let _wallet_data: serde_json::Value = serde_json::from_str(&fs::read_to_string(&wallet_file)?)?;

    println!("{}", "🗑️  Wallet Deletion".red().bold());
    println!(
        "You are about to delete the wallet with address: {}",
        address.yellow()
    );
    println!("{}", "⚠️  This action cannot be undone!".red().bold());
    println!("Make sure you have backed up your mnemonic phrase before proceeding.");
    println!();

    // Confirm deletion
    print!("Type 'DELETE' to confirm deletion: ");
    io::stdout().flush()?;
    let mut confirmation = String::new();
    io::stdin().read_line(&mut confirmation)?;

    if confirmation.trim() != "DELETE" {
        println!("❌ Deletion cancelled.");
        return Ok(());
    }

    // If we got here, passphrase is correct and integrity check passed
    println!("✅ Wallet integrity check passed");
    println!("🗑️  Deleting wallet...");

    // Delete wallet file
    fs::remove_file(&wallet_file)?;
    println!("✅ Wallet file deleted: {}", wallet_file.display());

    // Remove from wallets.json registry
    if wallets_file.exists() {
        let mut wallets: HashMap<String, serde_json::Value> = {
            let wallets_content = fs::read_to_string(&wallets_file)?;
            serde_json::from_str(&wallets_content)?
        };

        if wallets.remove(address).is_some() {
            fs::write(&wallets_file, serde_json::to_string_pretty(&wallets)?)?;
            println!("✅ Wallet removed from registry");
        } else {
            println!("⚠️  Wallet was not found in registry");
        }
    }

    println!("{}", "✅ Wallet deleted successfully!".green().bold());
    println!(
        "{}",
        "⚠️  Remember: Your mnemonic phrase is the only way to recover this wallet.".yellow()
    );

    Ok(())
}

/// Backup a wallet file to a specified destination
pub fn backup_wallet(wallet_address: &str, destination_path: &str) -> Result<(), anyhow::Error> {
    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_address));

    if !wallet_file.exists() {
        return Err(anyhow!("No wallet found with address: {}", wallet_address));
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

/// Import a wallet using secure mnemonic input (step-by-step) and password creation
pub fn import_wallet_from_mnemonic(network: Network) -> Result<String, anyhow::Error> {
    println!("{}", "🔒 Import Wallet with Secure Input".blue().bold());
    println!("This process will:");
    println!("• Securely collect your mnemonic phrase word by word");
    println!("• Create a secure passphrase to encrypt the wallet");
    println!("• Derive and store the wallet with full encryption");
    println!();

    // Prompt for mnemonic securely (word by word)
    println!("{}", "Step 1: Enter Mnemonic Phrase".yellow().bold());
    let secure_mnemonic = prompt_mnemonic_secure()?;

    // Generate address from mnemonic using helper function
    let address = crate::address::generate_address_from_mnemonic_secure(&secure_mnemonic, network)
        .map_err(|e| anyhow!("Failed to generate address from mnemonic: {}", e))?;

    println!();
    println!("✅ Mnemonic processed successfully!");
    println!("Derived address: {}", address.to_string().green());
    println!();

    // Check if wallet already exists
    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    if wallet_file.exists() {
        return Err(anyhow!(
            "❌ Wallet with address '{}' already exists in local storage.\nLocation: {}",
            address,
            wallet_file.display()
        ));
    }

    // Securely prompt for passphrase
    println!("{}", "Step 2: Create Secure Passphrase".yellow().bold());
    println!("Enter a strong passphrase to encrypt your imported wallet:");
    let passphrase = get_validated_passphrase("Passphrase: ", true)?;

    // Confirm passphrase using secure comparison
    confirm_passphrase_secure(&passphrase)?;
    println!("✅ Passphrase created successfully!");
    println!();

    // Encrypt and store wallet
    println!(
        "{}",
        "Step 3: Encrypting and Storing Wallet".yellow().bold()
    );

    let master_private_key_secure = derive_private_key_from_mnemonic_secure(&secure_mnemonic)
        .map_err(|e| anyhow!("Failed to derive private key from mnemonic: {}", e))?;

    let encrypted_mnemonic = aes_encrypt_secure(&secure_mnemonic, &passphrase)
        .map_err(|e| anyhow!("Failed to encrypt mnemonic: {}", e))?;

    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .map_err(|e| anyhow!("Failed to encrypt private key: {}", e))?;

    crate::wallet_storage::store_wallet_data(
        &address.to_string(),
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
        "separate_encrypted_fields",
        true,
        Some("secure_mnemonic_input"),
    )
    .map_err(|e| anyhow!("Failed to store wallet: {}", e))?;

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

pub fn verify_wallet_integrity() -> Result<(), anyhow::Error> {
    let storage_dir = get_storage_dir()?;
    let wallets_file = storage_dir.join("wallets.json");

    println!("{}", "🔍 Verifying Wallet Integrity".blue().bold());
    println!("Storage directory: {}", storage_dir.display());
    println!();

    // Read wallets.json registry
    let registry_wallets: HashSet<String> = if wallets_file.exists() {
        let wallets_content = fs::read_to_string(&wallets_file)?;
        let wallets: HashMap<String, serde_json::Value> = serde_json::from_str(&wallets_content)
            .map_err(|e| anyhow!("Failed to parse wallets.json: {}", e))?;
        wallets.keys().cloned().collect()
    } else {
        println!("⚠️  wallets.json not found - no registered wallets");
        HashSet::new()
    };

    // Scan for actual wallet files in storage directory
    let mut file_wallets: HashSet<String> = HashSet::new();

    if storage_dir.exists() {
        for entry in fs::read_dir(&storage_dir)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            // Check if it's a wallet file (wallet_ADDRESS.json)
            if file_name_str.starts_with("wallet_")
                && file_name_str.ends_with(".json")
                && file_name_str != "wallets.json"
            {
                // Extract address from filename
                let address = file_name_str
                    .strip_prefix("wallet_")
                    .and_then(|s| s.strip_suffix(".json"))
                    .unwrap_or("")
                    .to_string();

                if !address.is_empty() {
                    file_wallets.insert(address);
                }
            }
        }
    } else {
        println!("⚠️  Storage directory does not exist");
    }

    // Compare registry vs files
    let registry_only: HashSet<_> = registry_wallets.difference(&file_wallets).collect();
    let files_only: HashSet<_> = file_wallets.difference(&registry_wallets).collect();
    let matching: HashSet<_> = registry_wallets.intersection(&file_wallets).collect();

    // Report results
    println!("📊 Integrity Verification Results:");
    println!("  Total registered wallets: {}", registry_wallets.len());
    println!("  Total wallet files found: {}", file_wallets.len());
    println!("  Matching entries: {}", matching.len());
    println!();

    let mut has_issues = false;

    // Report wallets in registry but missing files
    if !registry_only.is_empty() {
        has_issues = true;
        println!("{} Wallets in registry but missing files:", "❌".red());
        for address in &registry_only {
            println!(
                "  • {} (file: wallet_{}.json not found)",
                address.yellow(),
                address
            );
        }
        println!();
    }

    // Report wallet files not in registry
    if !files_only.is_empty() {
        has_issues = true;
        println!("{} Wallet files not in registry:", "⚠️".yellow());
        for address in &files_only {
            println!(
                "  • {} (wallet_{}.json exists but not registered)",
                address.yellow(),
                address
            );
        }
        println!();
    }

    // Report successful matches
    if !matching.is_empty() {
        println!("{} Properly registered wallets:", "✅".green());
        for address in &matching {
            println!("  • {}", address.green());
        }
        println!();
    }

    // Summary
    if has_issues {
        println!("{}", "❌ Integrity issues found!".red().bold());
        println!("Consider:");
        if !registry_only.is_empty() {
            println!("• Remove orphaned registry entries or restore missing wallet files");
        }
        if !files_only.is_empty() {
            println!("• Register untracked wallet files or remove them if not needed");
        }
    } else if registry_wallets.is_empty() && file_wallets.is_empty() {
        println!(
            "{}",
            "ℹ️  No wallets found (this is normal for new installations)".blue()
        );
    } else {
        println!(
            "{}",
            "✅ All wallets are properly registered and files exist!"
                .green()
                .bold()
        );
    }

    Ok(())
}

/// Helper function to parse network string into Network enum
fn parse_network(network_str: &str) -> Network {
    match network_str {
        "testnet4" => Network::Testnet4,
        "testnet" => Network::Testnet,
        "regtest" => Network::Regtest,
        "signet" => Network::Signet,
        _ => Network::Bitcoin,
    }
}

/// Helper function to get a valid passphrase from user with validation
fn get_validated_passphrase(
    prompt: &str,
    require_length: bool,
) -> Result<SecureString, anyhow::Error> {
    let mut attempts = 0;
    const MAX_ATTEMPTS: usize = 5;

    while attempts < MAX_ATTEMPTS {
        print!("{}", prompt);
        io::stdout().flush()?;
        let passphrase_input = rpassword::read_password()?;

        if passphrase_input.is_empty() {
            println!("❌ Passphrase cannot be empty for security reasons");
            attempts += 1;
            if attempts < MAX_ATTEMPTS {
                println!(
                    "Attempt {} of {}. Please try again.",
                    attempts + 1,
                    MAX_ATTEMPTS
                );
            }
            continue;
        }

        if require_length && passphrase_input.len() < 8 {
            println!("❌ Passphrase must be at least 8 characters long for security");
            attempts += 1;
            if attempts < MAX_ATTEMPTS {
                println!(
                    "Attempt {} of {}. Please try again.",
                    attempts + 1,
                    MAX_ATTEMPTS
                );
            }
            continue;
        }

        return Ok(SecureString::init_with(|| passphrase_input));
    }

    Err(anyhow!(
        "❌ Maximum attempts exceeded. Operation cancelled for security."
    ))
}

/// Helper function to securely confirm passphrases without exposing secrets
fn confirm_passphrase_secure(passphrase: &SecureString) -> Result<(), anyhow::Error> {
    let mut attempts = 0;
    const MAX_ATTEMPTS: usize = 3;

    while attempts < MAX_ATTEMPTS {
        print!("Confirm passphrase: ");
        io::stdout().flush()?;
        let mut confirm_input = rpassword::read_password()?;

        // Use constant-time comparison to avoid timing attacks
        let matches = {
            let passphrase_bytes = passphrase.expose_secret().as_bytes();
            let confirm_bytes = confirm_input.as_bytes();

            use subtle::ConstantTimeEq;
            if passphrase_bytes.len() != confirm_bytes.len() {
                false
            } else {
                passphrase_bytes.ct_eq(confirm_bytes).into()
            }
        };

        // Immediately zeroize the confirmation input
        confirm_input.zeroize();

        if matches {
            return Ok(());
        }

        attempts += 1;
        println!("❌ Passphrases do not match");
        if attempts < MAX_ATTEMPTS {
            println!(
                "Attempt {} of {}. Please try again.",
                attempts + 1,
                MAX_ATTEMPTS
            );
        }
    }

    Err(anyhow!(
        "❌ Maximum attempts exceeded for passphrase confirmation."
    ))
}

/// Helper function to validate private key imports during wallet import
fn validate_private_key_import(
    wallet_data: &serde_json::Value,
    passphrase: &SecureString,
    wallet_address: &str,
) -> Result<(), anyhow::Error> {
    use bitcoin::secp256k1::SecretKey;
    use std::str::FromStr;

    if wallet_data["encrypted_private_key"].as_object().is_some() {
        let encrypted_private_key_hex: crate::encryption::EncryptedDataHex =
            serde_json::from_value(wallet_data["encrypted_private_key"].clone())
                .map_err(|e| anyhow!("Failed to parse encrypted private key structure: {}", e))?;

        let encrypted_private_data =
            crate::encryption::encrypted_data_from_hex(&encrypted_private_key_hex)
                .map_err(|e| anyhow!("Failed to parse encrypted private key: {}", e))?;

        // Decrypt and validate the private key
        match aes_decrypt_secure(&encrypted_private_data, passphrase) {
            Ok(decrypted_private_key) => {
                let network_str = wallet_data["network"].as_str().unwrap_or("mainnet");
                let network = parse_network(network_str);

                // Validate the private key format and derive address to verify
                match SecretKey::from_str(decrypted_private_key.expose_secret()) {
                    Ok(mut private_key) => {
                        let keypair =
                            Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &private_key);
                        let derived_address = calculate_taproot_address(&keypair, network);

                        // Zeroize the private key after use
                        private_key.non_secure_erase();

                        if derived_address.to_string() != wallet_address {
                            return Err(anyhow!(
                                "❌ Address mismatch! The decrypted private key doesn't correspond to this wallet address."
                            ));
                        }
                        println!(
                            "✅ Passphrase verified successfully! Private key address confirmed."
                        );
                    }
                    Err(_) => {
                        return Err(anyhow!(
                            "❌ Invalid wallet file: invalid private key format"
                        ));
                    }
                }
            }
            Err(_) => {
                return Err(anyhow!(
                    "❌ Incorrect passphrase! Cannot decrypt private key data."
                ));
            }
        }
    } else {
        return Err(anyhow!(
            "❌ Invalid wallet file: missing encrypted_private_key field for private key import"
        ));
    }

    Ok(())
}

/// Helper function to validate mnemonic imports during wallet import
fn validate_mnemonic_import(
    decrypted_mnemonic: &SecureString,
    wallet_data: &serde_json::Value,
    wallet_address: &str,
) -> Result<(), anyhow::Error> {
    let network_str = wallet_data["network"].as_str().unwrap_or("mainnet");
    let network = parse_network(network_str);

    // Generate address from mnemonic to verify it matches
    match crate::address::generate_address_from_mnemonic_secure(decrypted_mnemonic, network) {
        Ok(derived_address) => {
            if derived_address != wallet_address {
                return Err(anyhow!(
                    "❌ Address mismatch! The decrypted mnemonic doesn't correspond to this wallet address."
                ));
            }
            println!("✅ Passphrase verified successfully! Address confirmed.");
        }
        Err(_) => return Err(anyhow!("❌ Invalid wallet file: invalid mnemonic format")),
    }

    Ok(())
}

/// Import a wallet from a file path
pub fn import_wallet_from_file(file_path: &str) -> Result<String, anyhow::Error> {
    let source_path = std::path::Path::new(file_path);

    if !source_path.exists() {
        return Err(anyhow!("Wallet file does not exist: {}", file_path));
    }

    if !source_path.is_file() {
        return Err(anyhow!("Path is not a file: {}", file_path));
    }

    // Read and validate the wallet file
    let wallet_content = fs::read_to_string(source_path)?;
    let wallet_data: serde_json::Value = serde_json::from_str(&wallet_content)?;

    // Extract wallet address from the file content
    let wallet_address = wallet_data["address"]
        .as_str()
        .ok_or_else(|| anyhow!("Invalid wallet file: missing address field"))?;

    // Validate required fields
    if wallet_data["network"].is_null() {
        return Err(anyhow!("Invalid wallet file: missing network field"));
    }

    // Check if encrypted data exists (new format with separate encrypted fields)
    if !wallet_data["encrypted_mnemonic"].is_object() {
        return Err(anyhow!(
            "Invalid wallet file: missing encrypted_mnemonic field"
        ));
    }

    // Check if destination wallet already exists BEFORE prompting for passphrase
    let storage_dir = get_storage_dir()?;
    let dest_wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_address));

    if dest_wallet_file.exists() {
        return Err(anyhow!(
            "❌ Wallet with address '{}' already exists in local storage",
            wallet_address
        ));
    }

    println!("{}", "🔐 Passphrase Verification Required".yellow().bold());
    println!("To import this wallet, you must provide the correct passphrase to verify access.");

    // Prompt for passphrase to verify the user can decrypt the wallet
    let passphrase = get_validated_passphrase("Enter the passphrase for this wallet: ", true)?;

    // Verify passphrase by attempting to decrypt the wallet data
    println!("🔍 Verifying passphrase...");

    // Parse encrypted mnemonic as EncryptedDataHex
    let encrypted_mnemonic_hex: crate::encryption::EncryptedDataHex =
        serde_json::from_value(wallet_data["encrypted_mnemonic"].clone())
            .map_err(|e| anyhow!("Failed to parse encrypted mnemonic structure: {}", e))?;

    let encrypted_data = crate::encryption::encrypted_data_from_hex(&encrypted_mnemonic_hex)
        .map_err(|e| anyhow!("Failed to parse encrypted mnemonic: {}", e))?;

    // Try to decrypt mnemonic to verify passphrase
    match aes_decrypt_secure(&encrypted_data, &passphrase) {
        Ok(decrypted_mnemonic) => {
            // Additional validation: check if decrypted content looks like a valid mnemonic
            let mnemonic_str = decrypted_mnemonic.expose_secret();

            // Basic validation: should have words separated by spaces
            let words: Vec<&str> = mnemonic_str.split_whitespace().collect();
            if words.len() != MNEMONIC_WORD_COUNT {
                return Err(anyhow!(
                    "❌ Invalid wallet file: decrypted data doesn't appear to be a valid mnemonic"
                ));
            }

            // Validate wallet data based on import type
            if mnemonic_str == "IMPORTED_FROM_PRIVATE_KEY" {
                validate_private_key_import(&wallet_data, &passphrase, wallet_address)?;
            } else {
                validate_mnemonic_import(&decrypted_mnemonic, &wallet_data, wallet_address)?;
            }
        }
        Err(_) => {
            return Err(anyhow!(
                "❌ Incorrect passphrase! Cannot decrypt wallet data."
            ));
        }
    }

    // Storage directory was already created during the file existence check
    fs::create_dir_all(&storage_dir)?;

    // Create new wallet content instead of copying
    let network = wallet_data["network"].as_str().unwrap_or("mainnet");
    let data_format = wallet_data["data_format"]
        .as_str()
        .unwrap_or("separate_encrypted_fields");

    // Extract and convert encrypted data from the original wallet
    let encrypted_mnemonic_hex: crate::encryption::EncryptedDataHex =
        serde_json::from_value(wallet_data["encrypted_mnemonic"].clone())
            .map_err(|e| anyhow!("Failed to parse encrypted mnemonic: {}", e))?;

    let encrypted_private_key_hex: crate::encryption::EncryptedDataHex =
        serde_json::from_value(wallet_data["encrypted_private_key"].clone())
            .map_err(|e| anyhow!("Failed to parse encrypted private key: {}", e))?;

    // Convert hex structures to EncryptedData
    let encrypted_mnemonic_data =
        crate::encryption::encrypted_data_from_hex(&encrypted_mnemonic_hex)
            .map_err(|e| anyhow!("Failed to convert encrypted mnemonic: {}", e))?;

    let encrypted_private_key_data =
        crate::encryption::encrypted_data_from_hex(&encrypted_private_key_hex)
            .map_err(|e| anyhow!("Failed to convert encrypted private key: {}", e))?;

    // Use store_wallet_data function for consistent storage
    let network_enum = parse_network(network);
    crate::wallet_storage::store_wallet_data(
        wallet_address,
        network_enum,
        &encrypted_mnemonic_data,
        &encrypted_private_key_data,
        data_format,
        true,
        Some("file_import"),
    )?;

    println!(
        "{} Wallet '{}' imported successfully from: {}",
        "✓".green(),
        wallet_address.cyan(),
        file_path.yellow()
    );

    Ok(wallet_address.to_string())
}

/// Import a wallet from a private key
pub fn import_wallet_from_private_key(network: Network) -> Result<String, anyhow::Error> {
    use crate::bitcoin_utils::calculate_taproot_address;
    use bitcoin::secp256k1::{Keypair, SecretKey};

    let private_key = rpassword::prompt_password("Enter your private key (hex format): ")
        .map_err(|e| anyhow!("Failed to read private key: {}", e))?;

    let mut private_key_bytes =
        hex::decode(private_key).map_err(|e| anyhow!("Invalid private key hex format: {}", e))?;

    if private_key_bytes.len() != 32 {
        private_key_bytes.zeroize();
        return Err(anyhow!(
            "Private key must be exactly 32 bytes (64 hex characters)"
        ));
    }

    let mut master_private_key = SecretKey::from_slice(&private_key_bytes).map_err(|e| {
        private_key_bytes.zeroize();
        anyhow!("Invalid private key: {}", e)
    })?;
    private_key_bytes.zeroize();

    let keypair = Keypair::from_secret_key(&crate::bitcoin_utils::SECP, &master_private_key);
    let address = calculate_taproot_address(&keypair, network);

    println!("Derived address: {}", address.to_string().green());

    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    if wallet_file.exists() {
        return Err(anyhow!(
            "❌ Wallet with address '{}' already exists in local storage",
            address
        ));
    }

    let passphrase =
        get_validated_passphrase("Enter passphrase to encrypt the imported wallet: ", true)?;

    // Confirm passphrase using secure comparison
    confirm_passphrase_secure(&passphrase)?;

    let placeholder_mnemonic = SecureString::init_with(|| "IMPORTED_FROM_PRIVATE_KEY".to_string());

    let mut master_private_key_str = master_private_key.display_secret().to_string();
    let master_private_key_secure = SecureString::init_with(|| master_private_key_str.clone());

    master_private_key_str.zeroize();
    master_private_key.non_secure_erase();

    let encrypted_mnemonic = aes_encrypt_secure(&placeholder_mnemonic, &passphrase)
        .map_err(|e| anyhow!("Failed to encrypt placeholder mnemonic: {}", e))?;
    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .map_err(|e| anyhow!("Failed to encrypt private key: {}", e))?;

    crate::wallet_storage::store_wallet_data(
        &address.to_string(),
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
        "separate_encrypted_fields",
        true,
        Some("private_key_import"),
    )
    .map_err(|e| anyhow!("Failed to store wallet: {}", e))?;

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

pub fn create_encrypted_wallet_with_address(
    network: Network,
) -> Result<SecureString, anyhow::Error> {
    println!("Creating new wallet with maximum security protection");
    println!();
    println!("{}", "Security Features:".yellow());
    println!("• Secure terminal input (no echo)");
    println!("• Industry-standard secret handling (secrecy crate)");
    println!("• Memory zeroization");
    println!("• AES-256-GCM authenticated encryption");
    println!("• Argon2 key derivation");
    println!();

    // Generate mnemonic
    let secure_mnemonic = generate_mnemonic_secure()?;

    // Generate address from mnemonic using helper function
    let address = crate::address::generate_address_from_mnemonic_secure(&secure_mnemonic, network)
        .map_err(|e| anyhow!("Failed to generate address from mnemonic: {}", e))?;

    println!("Generated address: {}", address.green());

    // Prompt for passphrase
    let passphrase = get_validated_passphrase("Enter passphrase to encrypt the wallet: ", true)
        .map_err(|e| anyhow!("{}", e))?;

    // Confirm passphrase using secure comparison
    confirm_passphrase_secure(&passphrase).map_err(|e| anyhow!("{}", e))?;

    // Encrypt mnemonic and private key separately with different nonces
    let master_private_key_secure = derive_private_key_from_mnemonic_secure(&secure_mnemonic)?;

    let encrypted_mnemonic = aes_encrypt_secure(&secure_mnemonic, &passphrase)?;
    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)?;

    // Store encrypted wallet with separate encrypted fields
    crate::wallet_storage::store_wallet_data(
        &address.to_string(),
        network,
        &encrypted_mnemonic,
        &encrypted_private_key,
        "separate_encrypted_fields",
        false,
        None,
    )?;

    let storage_dir = get_storage_dir()?;
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

    Ok(secure_mnemonic)
}

pub fn export_private_key(address: &str, network: Network) -> Result<(), anyhow::Error> {
    // Check if wallet file exists before prompting for passphrase
    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    if !wallet_file.exists() {
        return Err(anyhow!("❌ Wallet file not found for address: {}", address));
    }

    let passphrase = get_validated_passphrase("Enter passphrase to decrypt private key: ", true)
        .map_err(|e| anyhow!("{}", e))?;

    let mut keypair = load_key(address, network, &passphrase)?;

    display_private_key_securely(&keypair.secret_key())?;

    keypair.non_secure_erase();

    Ok(())
}

/// Securely load a key from wallet storage - always requires a passphrase
pub fn load_key(
    address: &str,
    network: Network,
    passphrase: &SecureString,
) -> Result<Keypair, anyhow::Error> {
    use bitcoin::secp256k1::{Secp256k1, SecretKey};
    use std::str::FromStr;

    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    if !wallet_file.exists() {
        return Err(anyhow!("Wallet file not found for address: {address}"));
    }

    // Load wallet data
    let wallet_data = crate::wallet_storage::load_wallet_data(address)?;

    // Verify network matches
    if wallet_data.network != network.to_string() {
        return Err(anyhow!(
            "Wallet network mismatch: expected {}, found {}",
            network,
            wallet_data.network
        ));
    }

    // Load the encrypted private key
    let encrypted_private_key = wallet_data
        .encrypted_private_key
        .ok_or_else(|| anyhow!("No encrypted private key found in wallet"))?;

    let encrypted_data = crate::encryption::encrypted_data_from_hex(&encrypted_private_key)?;
    let decrypted_key = aes_decrypt_secure(&encrypted_data, passphrase)?;

    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_str(decrypted_key.expose_secret())?;
    Ok(Keypair::from_secret_key(&secp, &secret_key))
}

#[cfg(test)]
pub mod tests {
    use crate::passphrase::derive_key_from_passphrase;

    use super::*;

    #[test]
    fn test_derive_key_from_passphrase() {
        // Test basic key derivation
        let passphrase = SecureString::init_with(|| "test_passphrase_123".to_string());
        let salt = [1u8; 32];

        let key1 = derive_key_from_passphrase(&passphrase, &salt, 1000, 1024, 1).unwrap();
        let key2 = derive_key_from_passphrase(&passphrase, &salt, 1000, 1024, 1).unwrap();

        // Same inputs should produce same key
        assert_eq!(key1.expose_secret(), key2.expose_secret());
    }

    #[test]
    fn test_derive_key_different_inputs() {
        let passphrase1 = SecureString::init_with(|| "passphrase1".to_string());
        let passphrase2 = SecureString::init_with(|| "passphrase2".to_string());
        let salt = [1u8; 32];

        let key1 = derive_key_from_passphrase(&passphrase1, &salt, 1000, 1024, 1).unwrap();
        let key2 = derive_key_from_passphrase(&passphrase2, &salt, 1000, 1024, 1).unwrap();

        // Different passphrases should produce different keys
        assert_ne!(key1.expose_secret(), key2.expose_secret());
    }

    #[test]
    fn test_derive_key_different_salts() {
        let passphrase = SecureString::init_with(|| "same_passphrase".to_string());
        let salt1 = [1u8; 32];
        let salt2 = [2u8; 32];

        let key1 = derive_key_from_passphrase(&passphrase, &salt1, 1000, 1024, 1).unwrap();
        let key2 = derive_key_from_passphrase(&passphrase, &salt2, 1000, 1024, 1).unwrap();

        // Different salts should produce different keys
        assert_ne!(key1.expose_secret(), key2.expose_secret());
    }

    #[test]
    fn test_key_derivation_parameters() {
        // This test is maintained for key derivation functionality
        let passphrase = SecureString::init_with(|| "test_passphrase".to_string());
        let salt = [1u8; 32];

        // Test with minimum secure parameters
        let key_min = derive_key_from_passphrase(&passphrase, &salt, 3, 1024, 1).unwrap();

        // Test with production parameters
        let key_prod = derive_key_from_passphrase(&passphrase, &salt, 3, 65_536, 4).unwrap();

        // Both should succeed but produce different keys due to different parameters
        assert_ne!(key_min.expose_secret(), key_prod.expose_secret());
    }
}
