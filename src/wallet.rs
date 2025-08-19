use anyhow::anyhow;
use colored::Colorize;
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};

use crate::mnemonic::{
    derive_private_key_from_mnemonic_secure, load_mnemonic_secure, load_private_key_secure,
    prompt_secure_passphrase,
};
use crate::storage::get_storage_dir;

pub fn delete_wallet(address: &str) -> Result<(), anyhow::Error> {
    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
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

    // Prompt for passphrase to verify access
    let passphrase = prompt_secure_passphrase("Enter passphrase to verify wallet access: ")?;

    // Try to decrypt both mnemonic and private key to verify passphrase is correct
    let mnemonic = load_mnemonic_secure(address, passphrase.expose_secret())?;
    let stored_private_key = load_private_key_secure(address, passphrase.expose_secret())?;

    // Derive private key from mnemonic
    let derived_private_key = derive_private_key_from_mnemonic_secure(&mnemonic)?;

    // Compare stored private key with derived private key
    if stored_private_key != derived_private_key {
        return Err(anyhow!(
            "❌ Wallet integrity check failed! The mnemonic and private key do not match.\n\
            This could indicate wallet corruption or tampering."
        ));
    }

    // If we got here, passphrase is correct and integrity check passed
    println!("🔒 Passphrase verified successfully");
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

pub fn verify_wallet_integrity() -> Result<(), anyhow::Error> {
    use std::collections::HashSet;

    let storage_dir = get_storage_dir().map_err(|e| anyhow::Error::msg(e.to_string()))?;
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
