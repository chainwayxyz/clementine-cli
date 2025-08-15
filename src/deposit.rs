// Deposit-related commands and logic for Clementine CLI

use crate::backend::create_deposit_account;
use crate::bitcoin_utils::{
    calculate_deposit_address, confirm_private_key_storage, generate_key_and_taproot_address,
};
use crate::bitcoin_utils::{
    generate_keypair_and_taproot_address_from_private_key,
    sign_recovery_tx as utils_sign_recovery_tx,
};
use crate::config::CliConfig;
use crate::config::CliConfig;
use crate::parameters::get_citrea_deposit_params;
use crate::storage::{load_key, prompt_new_passphrase, prompt_unlock_passphrase, store_key};
use crate::withdrawal::{get_tx_details, get_txout_details};
use crate::{BitcoinAddress, CitreaAddress, parse_citrea_address};
use bitcoin::AddressType;
use bitcoin::consensus::deserialize;
use bitcoin::{Amount, FeeRate, OutPoint, Transaction, Txid};
use bitcoin::{Network, address::NetworkUnchecked};
use colored::*;
use std::str::FromStr;

pub fn parse_address(
    address: &str,
    network: Network,
) -> Result<BitcoinAddress, Box<dyn std::error::Error>> {
    let unchecked_address: BitcoinAddress<NetworkUnchecked> = address
        .parse()
        .map_err(|_| "Invalid Bitcoin address format")?;
    let address = unchecked_address.require_network(network)?;
    Ok(address)
}

/// Parse and validate taproot address for the specified network
pub fn parse_taproot_address(
    address: &str,
    network: Network,
) -> Result<BitcoinAddress, Box<dyn std::error::Error>> {
    let address = parse_address(address, network)?;

    // Verify it's a taproot (P2TR) address
    if address.address_type() != Some(AddressType::P2tr) {
        return Err("Address is not a taproot (P2TR) address".into());
    }

    Ok(address)
}

/// Generate a new recovery key and taproot address for deposit operations
pub fn generate_recovery_key(
    auto_yes: bool,
    private_key: Option<String>,
    network: Network,
    word_count: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Confirm with user about private key storage
    if !confirm_private_key_storage(auto_yes)? {
        println!("Operation cancelled by user.");
        return Ok(());
    }

    let (keypair, address) = if let Some(private_key) = private_key {
        generate_keypair_and_taproot_address_from_private_key(&private_key, network)
    } else {
        generate_key_and_taproot_address(network, 0, word_count)
    }?;

    // Prompt for passphrase to encrypt the key
    let secure_passphrase = prompt_new_passphrase()?;

    // Store the key securely
    let stored_address = store_key(&keypair, network, secure_passphrase.as_str())?;

    // Verify the stored address matches the generated one
    if stored_address != address {
        return Err("Address mismatch after storage".into());
    }

    println!("{} {}", "ADDRESS".cyan().bold(), address);
    println!("{} {}", "NETWORK".blue().bold(), network);

    Ok(())
}

/// Get deposit address from backend
pub fn get_deposit_address(
    citrea_address: &str,
    recovery_taproot_address: &str,
    config: &CliConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let citrea_address: CitreaAddress = parse_citrea_address(citrea_address)?;
    println!(
        "{} {}",
        "CITREA_ADDRESS (checksummed)".green().bold(),
        citrea_address,
    );
    let recovery_taproot_address = parse_taproot_address(recovery_taproot_address, config.network)?;

    // Call backend to create deposit account
    let deposit_address =
        create_deposit_account(&citrea_address, &recovery_taproot_address, config)?;

    println!("{} {}", "DEPOSIT_ADDRESS".green().bold(), deposit_address);

    let (calculated_deposit_address, _) =
        calculate_deposit_address(&citrea_address, &recovery_taproot_address, config)?;

    assert_eq!(deposit_address, calculated_deposit_address);

    println!(
        "{} {}",
        "Deposit address:".blue().bold(),
        calculated_deposit_address
    );
    Ok(())
}

pub async fn get_deposit_params(
    move_to_vault_txid: &str,
    config: &CliConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let move_to_vault_txid = Txid::from_str(move_to_vault_txid)?;
    // 2. Get the prepare tx details
    let (move_to_vault_tx, move_to_vault_block, move_to_vault_block_height) =
        get_tx_details(&move_to_vault_txid, config).await?;

    let move_to_vault_txout = get_txout_details(
        config,
        &move_to_vault_tx.input[0].previous_output.txid,
        move_to_vault_tx.input[0].previous_output.vout,
    )
    .await?;

    let deposit_params = get_citrea_deposit_params(
        move_to_vault_txout,
        &move_to_vault_tx,
        &move_to_vault_block,
        move_to_vault_block_height,
    )?;

    println!("{}", "Encoded deposit params:".blue().bold());
    println!("{}", hex::encode(deposit_params));

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn sign_recovery_tx(
    citrea_address: &str,
    recovery_taproot_address: &str,
    deposit_txid: &str,
    deposit_vout: u32,
    claim_address: &str,
    fee_rate: Option<u64>,
    amount: Option<f64>,
    config: &CliConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let citrea_addr: CitreaAddress = parse_citrea_address(citrea_address)?;
    let recovery_addr = parse_taproot_address(recovery_taproot_address, config.network)?;
    let claim_addr = BitcoinAddress::from_str(claim_address)?.require_network(config.network)?;
    let txid = Txid::from_str(deposit_txid)?;
    let outpoint = OutPoint {
        txid,
        vout: deposit_vout,
    };
    // Try loading key without passphrase first, if that fails, prompt for passphrase
    let keypair = match load_key(recovery_taproot_address, config.network, None) {
        Ok(keypair) => keypair,
        Err(_) => {
            // Key might be encrypted, prompt for passphrase
            println!("Key appears to be encrypted. Please enter the passphrase:");
            let secure_passphrase = prompt_unlock_passphrase()?;
            load_key(
                recovery_taproot_address,
                config.network,
                Some(secure_passphrase.as_str()),
            )?
        }
    };

    // Convert BTC amount to satoshis if provided
    let deposit_amount = match amount {
        Some(btc) => Some(Amount::from_btc(btc)?),
        None => None,
    };

    let fee_rate_opt = fee_rate.map(FeeRate::from_sat_per_vb_unchecked);
    let signed_tx = utils_sign_recovery_tx(
        &keypair,
        &citrea_addr,
        &recovery_addr,
        &outpoint,
        deposit_amount,
        &claim_addr,
        fee_rate_opt,
        config,
    )?;
    println!(
        "Signed Recovery Transaction: {}",
        hex::encode(bitcoin::consensus::serialize(&signed_tx))
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn verify_recovery_tx(
    recovery_tx: &str,
    citrea_address: &str,
    recovery_taproot_address: &str,
    amount: Option<f64>,
    config: &CliConfig,
) -> Result<(Txid, BitcoinAddress, Amount), Box<dyn std::error::Error>> {
    let recovery_tx: Transaction = deserialize(&hex::decode(recovery_tx)?)?;

    let (txid, address, amount) = crate::bitcoin_utils::verify_recovery_tx(
        &recovery_tx,
        &parse_citrea_address(citrea_address)?,
        &parse_taproot_address(recovery_taproot_address, config.network)?,
        amount.map(|amount| Amount::from_btc(amount).unwrap()),
        config,
    )?;

    println!(
        "{} Recovery transaction verification successful!",
        "SUCCESS".green().bold()
    );
    println!("{} {}", "Output address:".blue().bold(), address);
    println!("{} {} BTC", "Output amount:".blue().bold(), amount.to_btc());
    println!(
        "\n{} This transaction can be broadcast after 200 blocks from {}",
        "NOTE:".yellow().bold(),
        txid
    );

    Ok((txid, address, amount))
}

// TODO: Implement deposit.deposit_status

/// Export private key for a taproot address
pub fn export_private_key(
    taproot_address: &str,
    network: Network,
) -> Result<(), Box<dyn std::error::Error>> {
    let address = parse_taproot_address(taproot_address, network)?;

    let private_key = crate::storage::export_private_key(&address.to_string(), network)?;

    println!("{} {}", "ADDRESS".cyan().bold(), address);
    println!("{} {}", "NETWORK".blue().bold(), network);
    println!("{} {}", "PRIVATE_KEY".red().bold(), private_key);
    println!(
        "{} \"Keep this private key secure and never share it!\"",
        "WARNING".yellow().bold(),
    );

    Ok(())
}

/// List all stored keys
pub fn list_stored_keys() -> Result<(), Box<dyn std::error::Error>> {
    let keys = crate::storage::list_keys()?;

    if keys.is_empty() {
        println!("{} No keys found in storage", "INFO".yellow().bold());
        return Ok(());
    }

    println!("{} Stored keys:", "INFO".cyan().bold());
    println!();

    for (address, metadata) in keys {
        let network = metadata["network"].as_str().unwrap_or("unknown");
        let stored_at = metadata["stored_at"].as_str().unwrap_or("unknown");

        println!("{} {}", "ADDRESS".cyan().bold(), address);
        println!("{} {}", "NETWORK".blue().bold(), network);
        println!("{} {}", "STORED_AT".green().bold(), stored_at);
        println!();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::secp256k1::{Keypair, Secp256k1, SecretKey};

    #[test]
    fn test_parse_taproot_address_valid() {
        let addr_str = "bc1pdqrcrxa8vx6gy75mfdfj84puhxffh4fq46h3gkp6jxdd0vjcsdyspfxcv6";
        let addr = parse_taproot_address(addr_str, Network::Bitcoin).unwrap();
        assert_eq!(addr.address_type(), Some(AddressType::P2tr));
    }

    #[test]
    fn test_parse_taproot_address_invalid_type() {
        let non_taproot = "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx"; // P2WPKH
        assert!(parse_taproot_address(non_taproot, Network::Testnet4).is_err());
    }

    #[test]
    fn test_parse_taproot_address_wrong_network() {
        let mainnet_addr = "bc1pqqqqp399et2xygdj5xreqhjjvcmzhxw4aywxecjdzew6hylgvsesf3hn0c";
        assert!(parse_taproot_address(mainnet_addr, Network::Testnet4).is_err());
    }

    #[test]
    fn test_parse_taproot_address_invalid_format() {
        let invalid = "invalid_address";
        assert!(parse_taproot_address(invalid, Network::Testnet4).is_err());
    }

    // Integration tests for passphrase workflows
    #[test]
    fn test_sign_recovery_tx_with_encrypted_key() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_dir = temp_dir.path();

        // Create and store an encrypted key
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let network = Network::Testnet4;
        let passphrase = "test_recovery_passphrase";

        let recovery_address =
            crate::storage::store_key_with_base_dir(&keypair, network, passphrase, base_dir)
                .unwrap();

        // Test that we can load the key with correct passphrase
        let loaded_keypair = crate::storage::load_key_with_base_dir(
            &recovery_address.to_string(),
            network,
            Some(passphrase),
            base_dir,
        )
        .unwrap();
        assert_eq!(keypair.secret_key(), loaded_keypair.secret_key());

        // Test that loading fails with wrong passphrase
        let wrong_result = crate::storage::load_key_with_base_dir(
            &recovery_address.to_string(),
            network,
            Some("wrong_passphrase"),
            base_dir,
        );
        assert!(wrong_result.is_err());
        assert!(
            wrong_result
                .unwrap_err()
                .to_string()
                .contains("Decryption failed")
        );

        // Test that loading fails without passphrase (encrypted key)
        let no_pass_result = crate::storage::load_key_with_base_dir(
            &recovery_address.to_string(),
            network,
            None,
            base_dir,
        );
        assert!(no_pass_result.is_err());
        assert!(
            no_pass_result
                .unwrap_err()
                .to_string()
                .contains("encrypted and requires a passphrase")
        );

        // Temporary directory will be automatically cleaned up when temp_dir goes out of scope
    }

    #[test]
    fn test_recovery_key_generation_and_storage_integration() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_dir = temp_dir.path();

        // Create a keypair manually (simulating what generate_recovery_key would do)
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[2u8; 32]).unwrap();
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let network = Network::Testnet4;
        let passphrase = "integration_test_passphrase";

        // Store the key (this is what generate_recovery_key does internally)
        let stored_address =
            crate::storage::store_key_with_base_dir(&keypair, network, passphrase, base_dir)
                .unwrap();

        // Verify we can use this key in the recovery signing workflow
        let loaded_keypair = crate::storage::load_key_with_base_dir(
            &stored_address.to_string(),
            network,
            Some(passphrase),
            base_dir,
        )
        .unwrap();
        assert_eq!(keypair.secret_key(), loaded_keypair.secret_key());

        // Verify the address format is correct for taproot
        let address_result = parse_taproot_address(&stored_address.to_string(), network);
        assert!(address_result.is_ok());
        assert_eq!(
            address_result.unwrap().address_type(),
            Some(AddressType::P2tr)
        );

        // Temporary directory will be automatically cleaned up when temp_dir goes out of scope
    }

    #[test]
    fn test_key_storage_with_different_passphrases() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_dir = temp_dir.path();

        let secp = Secp256k1::new();
        let network = Network::Testnet4;

        // Test multiple keys with different passphrases
        let test_cases = vec![
            ("short_pass", [3u8; 32]),
            (
                "this_is_a_much_longer_passphrase_with_special_chars_!@#$%",
                [4u8; 32],
            ),
            ("パスワード", [5u8; 32]), // Unicode passphrase
        ];

        for (passphrase, seed) in test_cases {
            let secret_key = SecretKey::from_slice(&seed).unwrap();
            let keypair = Keypair::from_secret_key(&secp, &secret_key);

            // Store with the passphrase
            let address =
                crate::storage::store_key_with_base_dir(&keypair, network, passphrase, base_dir)
                    .unwrap();

            // Verify we can load it back
            let loaded_keypair = crate::storage::load_key_with_base_dir(
                &address.to_string(),
                network,
                Some(passphrase),
                base_dir,
            )
            .unwrap();
            assert_eq!(keypair.secret_key(), loaded_keypair.secret_key());

            // Verify wrong passphrase fails
            let wrong_result = crate::storage::load_key_with_base_dir(
                &address.to_string(),
                network,
                Some("definitely_wrong"),
                base_dir,
            );
            assert!(wrong_result.is_err());
        }

        // Temporary directory will be automatically cleaned up when temp_dir goes out of scope
    }

    #[test]
    fn test_key_encryption_security_properties() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_dir = temp_dir.path();

        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[6u8; 32]).unwrap();
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let network = Network::Testnet4;
        let passphrase = "security_test_passphrase";

        let address =
            crate::storage::store_key_with_base_dir(&keypair, network, passphrase, base_dir)
                .unwrap();

        // Read the stored file and verify it's actually encrypted
        let storage_dir = base_dir.join(".clementine").join("keys");
        let key_file = storage_dir.join(format!("key_{address}.json"));
        let file_content = std::fs::read_to_string(key_file).unwrap();

        // The file should not contain the raw private key
        let private_key_str = secret_key.display_secret().to_string();
        assert!(!file_content.contains(&private_key_str));

        println!(
            "{} Key stored securely at: {}",
            "INFO".blue().bold(),
            file_content
        );

        // The file should contain encrypted metadata
        assert!(file_content.contains("\"encrypted\": true"));
        assert!(file_content.contains("\"version\": 2"));
        assert!(file_content.contains("\"kdf\": \"argon2id\""));
        assert!(file_content.contains("\"cipher\": \"aes-256-gcm\""));
        assert!(file_content.contains("\"ciphertext\":"));
        assert!(file_content.contains("\"salt\":"));
        assert!(file_content.contains("\"nonce\":"));

        // Verify we can still load the key
        let loaded_keypair = crate::storage::load_key_with_base_dir(
            &address.to_string(),
            network,
            Some(passphrase),
            base_dir,
        )
        .unwrap();
        assert_eq!(keypair.secret_key(), loaded_keypair.secret_key());

        // Temporary directory will be automatically cleaned up when temp_dir goes out of scope
    }

    #[test]
    fn test_passphrase_timing_resistance() {
        // This test verifies that wrong passphrases still go through the full
        // key derivation process (not just failing fast), which helps prevent
        // timing attacks
        let temp_dir = tempfile::tempdir().unwrap();
        let base_dir = temp_dir.path();

        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[7u8; 32]).unwrap();
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let network = Network::Testnet4;
        let correct_passphrase = "timing_test_passphrase";

        let address = crate::storage::store_key_with_base_dir(
            &keypair,
            network,
            correct_passphrase,
            base_dir,
        )
        .unwrap();

        // Test with wrong passphrase - should still take reasonable time
        let start = std::time::Instant::now();
        let wrong_result = crate::storage::load_key_with_base_dir(
            &address.to_string(),
            network,
            Some("wrong_passphrase"),
            base_dir,
        );
        let duration = start.elapsed();

        // Should fail
        assert!(wrong_result.is_err());
        assert!(
            wrong_result
                .unwrap_err()
                .to_string()
                .contains("Decryption failed")
        );

        // Should take at least some time (indicating key derivation occurred)
        // This is a rough test - in a real scenario, both correct and incorrect
        // passphrases should take similar time for key derivation
        assert!(duration.as_millis() > 10); // Very conservative threshold

        // Temporary directory will be automatically cleaned up when temp_dir goes out of scope
    }
}
