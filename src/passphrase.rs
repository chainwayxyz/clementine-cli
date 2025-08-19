use argon2::Argon2;
use colored::Colorize;
use secrecy::ExposeSecret;

use crate::secure_structs::{SecureByteSlice, SecureString};

/// Derive an encryption key from a passphrase using Argon2id
pub fn derive_key_from_passphrase(
    passphrase: &SecureString,
    salt: &[u8],
    iterations: u32,
    memory: u32,
    parallelism: u32,
) -> Result<SecureByteSlice, Box<dyn std::error::Error>> {
    println!("Deriving encryption key from passphrase...");
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(memory, iterations, parallelism, Some(32))
            .map_err(|e| format!("Invalid Argon2 parameters: {e}"))?,
    );

    let mut key = [0u8; 32];

    argon2
        .hash_password_into(passphrase.expose_secret().as_bytes(), salt, &mut key)
        .map_err(|e| format!("Key derivation failed: {e}"))?;

    println!("Key derived successfully.");

    Ok(SecureByteSlice::init_with(|| key))
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

    let secure_passphrase = SecureString::init_with(|| passphrase);

    Ok(secure_passphrase)
}

/// Prompt user for a passphrase to unlock existing encrypted key
pub fn prompt_unlock_passphrase() -> Result<SecureString, Box<dyn std::error::Error>> {
    let passphrase = rpassword::prompt_password("Enter passphrase to unlock key: ")?;

    if passphrase.is_empty() {
        return Err("Passphrase cannot be empty".into());
    }

    Ok(SecureString::init_with(|| passphrase))
}
