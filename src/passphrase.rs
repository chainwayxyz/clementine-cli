//! Passphrase handling and Argon2id key derivation
//!
//! This module implements secure passphrase collection and key derivation using Argon2id.
//! The KDF transforms user passphrases into 256-bit keys for AES encryption with memory-hard
//! computation to resist GPU/ASIC attacks.
//!
//! ## What is Argon2id?
//!
//! Argon2id is a memory-hard password hashing algorithm and the winner of the Password Hashing
//! Competition (PHC). It's specifically designed to resist attacks from specialized hardware
//! like GPUs and ASICs by requiring large amounts of memory during computation.
//!
//! ### Algorithm Properties
//!
//! - **Memory-Hard Function**: Requires significant RAM (configurable) during computation,
//!   making parallel attacks on specialized hardware economically infeasible
//! - **Hybrid Design**: Combines Argon2i (data-independent access) and Argon2d (data-dependent access)
//!   for optimal security against both side-channel and GPU attacks
//! - **Tunable Parameters**: Time cost, memory cost, and parallelism can be adjusted based on
//!   security requirements and available resources
//! - **Cryptographically Secure**: Resistant to rainbow table attacks, time-memory trade-offs,
//!   and length extension attacks
//!
//! ### Why Argon2id for Key Derivation?
//!
//! 1. **GPU Resistance**: High memory requirements make GPU-based brute force attacks expensive
//! 2. **ASIC Resistance**: Memory-hard property prevents efficient custom hardware implementations
//! 3. **Side-Channel Protection**: Hybrid mode provides resistance to timing attacks
//! 4. **Standardized**: RFC 9106 compliant with widespread cryptographic review
//! 5. **Deterministic**: Same input always produces same output (essential for encryption keys)
//!
//! ### Security Model
//!
//! The security of Argon2id depends on:
//! - **Time Cost**: Number of iterations (computational difficulty)
//! - **Memory Cost**: Amount of RAM required (hardware attack resistance)
//! - **Salt**: Unique random value preventing rainbow table attacks
//! - **Parallelism**: Number of threads (can improve performance without reducing security)
//!
//! In this implementation, we use conservative parameters that balance security with
//! interactive performance for command-line wallet operations.

use argon2::Argon2;
use colored::Colorize;
use secrecy::ExposeSecret;
use zeroize::Zeroize;

use crate::errors::BridgeCliError;
use crate::secure_structs::{SecureByteSlice, SecureString};

/// Derives a 256-bit AES key from passphrase using Argon2id
///
/// Uses memory-hard Argon2id algorithm to transform passphrase + salt into encryption key.
/// Same passphrase + salt always produces the same key (required for decryption).
pub fn derive_key_from_passphrase(
    passphrase: &SecureString,
    salt: &[u8],
    iterations: u32,
    memory: u32,
    parallelism: u32,
) -> Result<SecureByteSlice, BridgeCliError> {
    println!("Deriving encryption key from passphrase...");

    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(memory, iterations, parallelism, Some(32))
            .map_err(|e| BridgeCliError::InvalidArgon2Parameters(e.to_string()))?,
    );

    let mut key = [0u8; 32];

    // Passphrase + salt → 32-byte key via memory-hard computation
    argon2
        .hash_password_into(passphrase.expose_secret().as_bytes(), salt, &mut key)
        .map_err(|e| BridgeCliError::KeyDerivationError(e.to_string()))?;

    println!("Key derived successfully.");

    Ok(SecureByteSlice::init_with(|| key))
}

/// Prompt user for a passphrase with confirmation for new keys
pub fn prompt_passphrase() -> Result<SecureString, BridgeCliError> {
    println!("{}", "Passphrase protection:".blue().bold());
    println!("Enter a passphrase to encrypt your private key.");

    let passphrase = rpassword::prompt_password("Enter passphrase: ")?;

    if passphrase.is_empty() {
        return Err(BridgeCliError::EmptyPassphrase);
    }

    // Validate passphrase strength
    if passphrase.len() < 8 {
        return Err(BridgeCliError::PassphraseTooShort);
    }

    // Confirm passphrase
    let mut confirm = rpassword::prompt_password("Confirm passphrase: ")?;

    if passphrase != confirm {
        return Err(BridgeCliError::PassphraseMismatch);
    }

    confirm.zeroize(); // Clear confirmation from memory

    println!(
        "{} Private key will be encrypted with AES-256-GCM",
        "SECURE".green().bold()
    );

    let secure_passphrase = SecureString::init_with(|| passphrase);

    Ok(secure_passphrase)
}

/// Prompt user for a passphrase to unlock existing encrypted key
pub fn prompt_unlock_passphrase() -> Result<SecureString, BridgeCliError> {
    let passphrase = rpassword::prompt_password("Enter passphrase to unlock key: ")?;

    if passphrase.is_empty() {
        return Err(BridgeCliError::EmptyPassphrase);
    }

    Ok(SecureString::init_with(|| passphrase))
}
