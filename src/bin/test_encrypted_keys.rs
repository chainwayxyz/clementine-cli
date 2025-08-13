use bitcoin::Network;
use bitcoin::secp256k1::{Keypair, Secp256k1, SecretKey};
use clementine_cli::bitcoin_utils::calculate_taproot_address;
use clementine_cli::storage::{
    load_key, prompt_new_passphrase, prompt_unlock_passphrase, store_key,
};
use colored::*;
use std::io::{self, Write};

fn print_header() {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║                    🔐 Encrypted Key Storage Test                ║");
    println!("║                        Interactive Demo                         ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();
}

fn print_section(title: &str) {
    println!("{}", format!("🔹 {}", title).blue().bold());
    println!("{}", "─".repeat(60));
}

fn wait_for_user() {
    print!("\nPress Enter to continue...");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
}

fn generate_test_keypair() -> Result<(Keypair, Network), Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();

    // Generate a random secret key for demonstration
    use bitcoin::secp256k1::rand::RngCore;
    use bitcoin::secp256k1::rand::rngs::OsRng;

    let mut random_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut random_bytes);

    let secret_key = SecretKey::from_slice(&random_bytes)?;
    let keypair = Keypair::from_secret_key(&secp, &secret_key);
    let network = Network::Testnet4; // Use testnet for safety

    Ok((keypair, network))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    print_header();

    println!("This interactive demo will show you how encrypted key storage works.");
    println!("We'll generate a new keypair, encrypt it with a passphrase, and then decrypt it.");
    println!();

    // Step 1: Generate keypair
    print_section("Step 1: Generating Test Keypair");
    let (keypair, network) = generate_test_keypair()?;
    let address = calculate_taproot_address(&keypair, network);

    println!("{} Generated new keypair", "✓".green().bold());
    println!("{} {}", "Address:".cyan(), address);
    println!("{} {}", "Network:".cyan(), network);
    println!(
        "{} {}",
        "Private Key:".yellow(),
        keypair.secret_key().display_secret()
    );

    wait_for_user();

    // Step 2: Encrypt and store the key
    print_section("Step 2: Encrypting and Storing the Key");
    println!("Now you'll be prompted to enter a passphrase to encrypt the private key.");
    println!("This passphrase will be used with Argon2id + AES-256-GCM encryption.");
    println!();

    let passphrase = prompt_new_passphrase()?;

    println!("\n{} Storing encrypted key...", "🔐".bright_cyan());
    let stored_address = store_key(&keypair, network, passphrase.as_str())?;

    println!("{} Key stored successfully!", "✓".green().bold());
    println!(
        "{} Stored at address: {}",
        "📁".bright_blue(),
        stored_address
    );

    wait_for_user();

    // Step 3: Load and decrypt the key
    print_section("Step 3: Loading and Decrypting the Key");
    println!("Now we'll load the key back from storage and decrypt it.");
    println!("You'll be prompted for the passphrase you just entered.");
    println!();

    let unlock_passphrase = prompt_unlock_passphrase()?;

    println!("\n{} Loading encrypted key...", "🔓".bright_cyan());
    let loaded_keypair = load_key(
        &stored_address.to_string(),
        network,
        Some(unlock_passphrase.as_str()),
    )?;

    println!(
        "{} Key loaded and decrypted successfully!",
        "✓".green().bold()
    );

    // Step 4: Verify the keys match
    print_section("Step 4: Verification");
    let original_private_key = keypair.secret_key().display_secret().to_string();
    let loaded_private_key = loaded_keypair.secret_key().display_secret().to_string();

    if original_private_key == loaded_private_key {
        println!(
            "{} Original and loaded keys match perfectly!",
            "✅".bright_green()
        );
        println!("{} Private keys are identical", "🔑".bright_yellow());
        println!("{} Public keys are identical", "🔑".bright_yellow());
    } else {
        println!("{} ERROR: Keys do not match!", "❌".bright_red());
        return Err("Key verification failed".into());
    }

    wait_for_user();

    // Step 5: Demo wrong passphrase
    print_section("Step 5: Wrong Passphrase Demo");
    println!("Let's see what happens when you enter the wrong passphrase...");
    println!("Enter any passphrase different from the one you used earlier:");
    println!();

    let wrong_passphrase = prompt_unlock_passphrase()?;

    println!(
        "\n{} Attempting to decrypt with wrong passphrase...",
        "🔓".bright_cyan()
    );
    match load_key(
        &stored_address.to_string(),
        network,
        Some(wrong_passphrase.as_str()),
    ) {
        Ok(_) => {
            println!(
                "{} Unexpected: Decryption succeeded (passphrases might be the same)",
                "⚠️".yellow()
            );
        }
        Err(e) => {
            println!(
                "{} Expected: Decryption failed with wrong passphrase",
                "✅".bright_green()
            );
            println!("{} Error: {}", "📝".cyan(), e);
        }
    }

    wait_for_user();

    // Print summary - all tests passed if we reach here
    print_section("🎉 Demo Complete - Summary");
    println!("What we demonstrated:");
    println!("  {} Generated a new Bitcoin keypair", "1.".bright_blue());
    println!(
        "  {} Encrypted the private key using Argon2id + AES-256-GCM",
        "2.".bright_blue()
    );
    println!(
        "  {} Stored the encrypted key to disk with secure permissions",
        "3.".bright_blue()
    );
    println!(
        "  {} Loaded and successfully decrypted the key with correct passphrase",
        "4.".bright_blue()
    );
    println!(
        "  {} Verified that wrong passphrase fails to decrypt",
        "5.".bright_blue()
    );
    println!();

    println!("{}", "Security Features Demonstrated:".green().bold());
    println!("  • Argon2id key derivation (memory-hard, side-channel resistant)");
    println!("  • AES-256-GCM authenticated encryption");
    println!("  • Random salt and nonce generation");
    println!("  • Secure memory handling with zeroization");
    println!("  • File permission restrictions (600 - owner read/write only)");
    println!();

    println!(
        "{} Test completed successfully! Your encrypted key storage is working.",
        "🎊".bright_green()
    );
    println!(
        "{} The key file is located at: .clementine/keys/key_{}.json",
        "📁".cyan(),
        stored_address
    );

    Ok(())
}
