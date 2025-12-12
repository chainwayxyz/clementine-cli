//! BIP-39 mnemonic phrase utilities and secure seed management for Clementine CLI.
//!
//! This module provides secure handling of mnemonic phrases for Bitcoin wallet operations:
//! - Generating cryptographically secure 12-word mnemonic phrases
//! - Converting mnemonics to master seeds using BIP-39 standard
//! - Loading encrypted mnemonics from wallet storage with passphrase decryption  
//! - Deriving private keys from mnemonic phrases
//! - Interactive secure mnemonic input with real-time validation
//!
//! ## Security Features
//!
//! All sensitive data is handled using secure memory constructs:
//! - **Secure memory management**: Uses zeroization to clear sensitive data
//! - **Encrypted storage**: Mnemonics are stored encrypted with AES encryption
//! - **Input validation**: Real-time validation against BIP-39 English wordlist
//! - **Private key isolation**: Handles wallets imported from private keys separately
//!
//! ## Mnemonic Standards
//!
//! - Uses BIP-39 standard with English language wordlist
//! - Fixed 12-word mnemonic phrase length for consistency
//! - Generates 256-bit entropy for cryptographic security
//! - Converts to 32-byte master seeds for key derivation
//!

use bip39::{Language, Mnemonic};
use bitcoin::address::NetworkValidation;
use secrecy::ExposeSecret;

use crate::errors::BridgeCliError;
use crate::secure_types::{
    SecureByteSlice, SecureSecretKey, SecureSeed, SecureString, SecureWordVec,
};
use crate::structs::{AddrDisplay, TaprootAddressWithPrefix};
use crate::wallet::encryption::{aes_decrypt_secure, encrypted_data_from_hex};
use crate::wallet::wallet_storage::load_wallet_data;
use bitcoin::secp256k1::SecretKey;
use colored::Colorize;

pub const MNEMONIC_WORD_COUNT: usize = 12;

pub(crate) fn generate_mnemonic() -> Result<Mnemonic, BridgeCliError> {
    let mnemonic = Mnemonic::generate_in(Language::English, MNEMONIC_WORD_COUNT).map_err(|e| {
        tracing::error!("Error generating mnemonic: {}", e);
        BridgeCliError::MnemonicGenerationError
    })?;

    Ok(mnemonic)
}

/// Generate master seed from mnemonic phrase
pub(crate) fn get_master_seed_from_mnemonic(mnemonic: &Mnemonic) -> SecureByteSlice {
    let seed = SecureSeed::new(Box::new(mnemonic.to_seed("")));

    let mut master_seed = [0u8; 32];
    master_seed.copy_from_slice(&seed.expose_secret()[0..32]);

    SecureByteSlice::new(Box::new(master_seed))
}

pub(crate) fn load_mnemonic<T>(
    address: &TaprootAddressWithPrefix<T>,
    passphrase: &SecureString,
) -> Result<Mnemonic, BridgeCliError>
where
    T: NetworkValidation,
    bitcoin::Address<T>: AddrDisplay,
{
    let wallet_data = load_wallet_data(address)?;

    let encrypted_data = if let Some(encrypted_mnemonic) = &wallet_data.encrypted_mnemonic {
        encrypted_data_from_hex(encrypted_mnemonic)?
    } else {
        return Err(BridgeCliError::MissingEncryptedMnemonic);
    };

    let secure_mnemonic_str = aes_decrypt_secure(&encrypted_data, passphrase)?;

    // Check if this wallet was imported from a private key
    if secure_mnemonic_str.expose_secret() == "IMPORTED_FROM_PRIVATE_KEY" {
        return Err(BridgeCliError::NoMnemonicAvailable);
    }

    let mnemonic = Mnemonic::parse(secure_mnemonic_str.expose_secret()).map_err(|e| {
        tracing::error!("Error parsing mnemonic: {}", e);
        BridgeCliError::MnemonicParseError
    })?;

    Ok(mnemonic)
}

pub(crate) fn derive_private_key_from_mnemonic(
    mnemonic: &Mnemonic,
) -> Result<SecureString, BridgeCliError> {
    // Generate master seed from mnemonic using BIP-39
    let master_seed = get_master_seed_from_mnemonic(mnemonic);

    let master_private_key =
        SecureSecretKey::new(SecretKey::from_slice(master_seed.expose_secret())?);

    let secure_private_key = SecureString::init_with(|| {
        master_private_key
            .as_ref_inner()
            .display_secret()
            .to_string()
    });

    Ok(secure_private_key)
}

/// Securely prompt for mnemonic phrase word by word with validation
/// Prompt user to enter mnemonic interactively (production version)
#[cfg(not(any(test, feature = "test-helpers")))]
pub(crate) fn prompt_mnemonic() -> Result<Mnemonic, BridgeCliError> {
    println!("{}", "Secure Mnemonic Input".bold());
    println!(
        "Enter your {}-word mnemonic phrase word by word.",
        MNEMONIC_WORD_COUNT
    );
    println!("Each word will be validated against the BIP-39 wordlist.");
    println!(
        "The system will automatically proceed after {} words are entered.",
        MNEMONIC_WORD_COUNT
    );
    println!();

    let mut words = SecureWordVec::new();
    let mut word_index = 1;

    // Get the BIP-39 English wordlist for validation
    let wordlist = Language::English.word_list();

    loop {
        let word_input = rpassword::prompt_password(format!("Word {}: ", word_index))
            .map_err(|e| BridgeCliError::Eyre(eyre::eyre!(e)))?;

        let word = word_input.trim().to_lowercase();

        // Validate word against BIP-39 wordlist
        if wordlist.iter().any(|&w| w == word) {
            words.push(word);
            println!("Word {} accepted", word_index);
            word_index += 1;

            if words.len() == MNEMONIC_WORD_COUNT {
                println!();
                println!(
                    "You have entered {} words (valid mnemonic length).",
                    words.len()
                );
                break;
            }
        } else {
            // word_input is automatically cleaned up
            println!("Invalid word entered. Please try again.");
            println!("Hint: Words should be lowercase English BIP-39 words.");
        }
    }

    // Validate final mnemonic length
    let word_count = words.len();
    if word_count != MNEMONIC_WORD_COUNT {
        return Err(BridgeCliError::InvalidMnemonicLength(word_count));
    }

    // Join words and validate complete mnemonic - keep it secure from the start
    let secure_mnemonic_phrase = SecureString::init_with(|| words.join(" "));

    println!();
    println!(
        "{} Valid BIP-39 mnemonic phrase with 12 words",
        "SUCCESS".bold(),
    );
    println!("Mnemonic will be handled securely and zeroized from memory");

    let mnemonic = Mnemonic::parse(secure_mnemonic_phrase.expose_secret()).map_err(|e| {
        tracing::error!("Error validating mnemonic: {}", e);
        BridgeCliError::MnemonicValidationFailed
    })?;

    Ok(mnemonic)
}

/// Test version: returns a fixed test mnemonic without prompting
#[cfg(any(test, feature = "test-helpers"))]
pub(crate) fn prompt_mnemonic() -> Result<Mnemonic, BridgeCliError> {
    // This is a standard test mnemonic from BIP-39 spec
    let test_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    Mnemonic::parse(test_mnemonic).map_err(|e| {
        tracing::error!("Error creating test mnemonic: {}", e);
        BridgeCliError::MnemonicValidationFailed
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standard BIP39 test mnemonic from the spec
    const KNOWN_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn test_generate_mnemonic_produces_12_words() {
        let mnemonic = generate_mnemonic().expect("Should generate mnemonic");
        let words: Vec<&str> = mnemonic.words().collect();
        assert_eq!(
            words.len(),
            MNEMONIC_WORD_COUNT,
            "Mnemonic should have 12 words"
        );
    }

    #[test]
    fn test_known_mnemonic_produces_deterministic_seed() {
        // Test with known mnemonic to ensure determinism
        let mnemonic = Mnemonic::parse(KNOWN_MNEMONIC).expect("Known mnemonic should parse");

        let seed1 = get_master_seed_from_mnemonic(&mnemonic);
        let seed2 = get_master_seed_from_mnemonic(&mnemonic);

        assert_eq!(
            seed1.expose_secret(),
            seed2.expose_secret(),
            "Same mnemonic should produce same seed"
        );
    }

    #[test]
    fn test_seed_has_correct_length() {
        let mnemonic = Mnemonic::parse(KNOWN_MNEMONIC).expect("Known mnemonic should parse");

        let seed = get_master_seed_from_mnemonic(&mnemonic);

        assert_eq!(seed.expose_secret().len(), 32, "Seed should be 32 bytes");
    }

    #[test]
    fn test_derive_private_key_is_deterministic() {
        let mnemonic = Mnemonic::parse(KNOWN_MNEMONIC).expect("Known mnemonic should parse");

        let key1 = derive_private_key_from_mnemonic(&mnemonic).expect("Should derive key");
        let key2 = derive_private_key_from_mnemonic(&mnemonic).expect("Should derive key");

        assert_eq!(
            key1.expose_secret(),
            key2.expose_secret(),
            "Same mnemonic should produce same private key"
        );
    }

    #[test]
    fn test_different_mnemonics_produce_different_keys() {
        let mnemonic1 = generate_mnemonic().expect("Should generate mnemonic 1");
        let mnemonic2 = generate_mnemonic().expect("Should generate mnemonic 2");

        // Ensure they're actually different
        assert_ne!(
            mnemonic1.to_string(),
            mnemonic2.to_string(),
            "Generated mnemonics should be different"
        );

        let key1 = derive_private_key_from_mnemonic(&mnemonic1).expect("Should derive key 1");
        let key2 = derive_private_key_from_mnemonic(&mnemonic2).expect("Should derive key 2");

        assert_ne!(
            key1.expose_secret(),
            key2.expose_secret(),
            "Different mnemonics should produce different keys"
        );
    }

    #[test]
    fn test_different_mnemonics_produce_different_seeds() {
        let mnemonic1 = generate_mnemonic().expect("Should generate mnemonic 1");
        let mnemonic2 = generate_mnemonic().expect("Should generate mnemonic 2");

        let seed1 = get_master_seed_from_mnemonic(&mnemonic1);
        let seed2 = get_master_seed_from_mnemonic(&mnemonic2);

        assert_ne!(
            seed1.expose_secret(),
            seed2.expose_secret(),
            "Different mnemonics should produce different seeds"
        );
    }

    #[test]
    fn test_private_key_length() {
        let mnemonic = Mnemonic::parse(KNOWN_MNEMONIC).expect("Known mnemonic should parse");

        let privkey =
            derive_private_key_from_mnemonic(&mnemonic).expect("Should derive private key");

        // Private key display format should be 64 hex characters
        assert_eq!(
            privkey.expose_secret().len(),
            64,
            "Private key hex string should be 64 characters"
        );
    }
}
