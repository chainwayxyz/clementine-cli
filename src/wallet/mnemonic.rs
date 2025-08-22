use bip39::{Language, Mnemonic};
use secrecy::ExposeSecret;
use zeroize::Zeroize;

use crate::errors::BridgeCliError;
use crate::secure_display::display_mnemonic_securely;
use crate::structs::SecureString;
use crate::wallet::encryption::{aes_decrypt_secure, encrypted_data_from_hex};
use crate::wallet::passphrase::prompt_unlock_passphrase;
use crate::wallet::wallet_storage::load_wallet_data;
use bitcoin::secp256k1::SecretKey;
use colored::Colorize;

pub const MNEMONIC_WORD_COUNT: usize = 12;

pub fn show_mnemonic_secure(address: &str) -> Result<(), BridgeCliError> {
    let passphrase = prompt_unlock_passphrase()?;
    let mnemonic = load_mnemonic_secure(address, &passphrase)?;
    display_mnemonic_securely(&mnemonic)?;

    Ok(())
}

pub(crate) fn generate_mnemonic_secure() -> Result<SecureString, BridgeCliError> {
    let mut mnemonic = Mnemonic::generate_in(Language::English, MNEMONIC_WORD_COUNT)
        .map_err(|e| BridgeCliError::MnemonicGenerationError(e.to_string()))?;

    let safe_mnemonic = SecureString::init_with(|| mnemonic.to_string());

    mnemonic.zeroize();

    Ok(safe_mnemonic)
}

/// Generate master seed from mnemonic phrase
pub(crate) fn get_master_seed_from_mnemonic(
    mnemonic_phrase: &SecureString,
) -> Result<[u8; 32], BridgeCliError> {
    let mut mnemonic = Mnemonic::parse(mnemonic_phrase.expose_secret())
        .map_err(|e| BridgeCliError::MnemonicParseError(e.to_string()))?;

    // Generate seed (64 bytes)
    let mut seed = mnemonic.to_seed("");

    // Make this safe - secure
    let mut master_seed = [0u8; 32];
    master_seed.copy_from_slice(&seed[0..32]);

    mnemonic.zeroize();
    seed.zeroize();

    Ok(master_seed)
}

fn load_mnemonic_secure(
    wallet_name: &str,
    passphrase: &SecureString,
) -> Result<SecureString, BridgeCliError> {
    let wallet_data = load_wallet_data(wallet_name)?;

    let encrypted_data = if let Some(encrypted_mnemonic) = &wallet_data.encrypted_mnemonic {
        encrypted_data_from_hex(encrypted_mnemonic)?
    } else {
        return Err(BridgeCliError::MissingEncryptedMnemonic);
    };

    let secure_mnemonic = aes_decrypt_secure(&encrypted_data, passphrase)?;

    Ok(secure_mnemonic)
}

pub(crate) fn derive_private_key_from_mnemonic_secure(
    mnemonic: &SecureString,
) -> Result<SecureString, BridgeCliError> {
    // Generate master seed from mnemonic using BIP-39
    let master_seed = get_master_seed_from_mnemonic(mnemonic)?;

    let mut master_private_key = SecretKey::from_slice(&master_seed)?;

    let secure_private_key =
        SecureString::init_with(|| master_private_key.display_secret().to_string());

    master_private_key.non_secure_erase();

    Ok(secure_private_key)
}

/// Securely prompt for mnemonic phrase word by word with validation
pub(crate) fn prompt_mnemonic_secure() -> Result<SecureString, BridgeCliError> {
    println!("{}", "Secure Mnemonic Input".blue().bold());
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

    let mut words: Vec<String> = Vec::new();
    let mut word_index = 1;

    // Get the BIP-39 English wordlist for validation
    let wordlist = Language::English.word_list();

    loop {
        let mut word =
            rpassword::prompt_password(format!("Word {}: ", word_index.to_string().cyan()))
                .map_err(|e| BridgeCliError::Eyre(eyre::eyre!(e)))?
                .trim()
                .to_lowercase();

        // Validate word against BIP-39 wordlist
        if wordlist.iter().any(|&w| w == word) {
            words.push(word.clone());
            println!("Word {} accepted", word_index);
            word_index += 1;
            word.zeroize(); // Clear the word from memory

            // Check if we have a valid mnemonic length and offer to finish
            if words.len() == MNEMONIC_WORD_COUNT {
                println!();
                println!(
                    "You have entered {} words (valid mnemonic length).",
                    words.len().to_string().green()
                );
                break;
            }
        } else {
            word.zeroize(); // Clear invalid word from memory
            println!("Invalid word entered. Please try again.");
            println!("Hint: Words should be lowercase English BIP-39 words.");
        }
    }

    // Validate final mnemonic length
    let word_count = words.len();
    if word_count != MNEMONIC_WORD_COUNT {
        for mut word in words {
            word.zeroize();
        }
        return Err(BridgeCliError::InvalidMnemonicLength(word_count));
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
                "{} Valid BIP-39 mnemonic phrase with {} words",
                "SUCCESS".green().bold(),
                mnemonic_phrase.split_whitespace().count()
            );
            println!("Mnemonic will be handled securely and zeroized from memory");

            // Create secure string
            let secure_mnemonic = SecureString::init_with(|| mnemonic_phrase);

            mnemonic.zeroize();

            Ok(secure_mnemonic)
        }
        Err(e) => {
            mnemonic_phrase.zeroize();
            // This shouldn't happen since we validated each word, but safety check
            Err(BridgeCliError::MnemonicValidationFailed(e.to_string()))
        }
    }
}
