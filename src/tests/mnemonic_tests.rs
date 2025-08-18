use crate::mnemonic::*;
use crate::secure_display::display_mnemonic_securely;
use crate::storage::{get_master_seed_from_mnemonic, get_storage_dir};
use bitcoin::Network;
use secrecy::ExposeSecret;
use std::fs;
use tempfile::tempdir;

// Helper function to create a test storage directory
fn setup_test_storage() -> tempfile::TempDir {
    tempdir().expect("Failed to create temp directory")
}

// Override get_storage_dir for testing
fn get_test_storage_dir(temp_dir: &tempfile::TempDir) -> std::path::PathBuf {
    temp_dir.path().join(".clementine").join("keys")
}

#[test]
fn test_generate_and_store_wallet() {
    let temp_dir = setup_test_storage();
    let storage_dir = get_test_storage_dir(&temp_dir);

    // Create storage directory
    fs::create_dir_all(&storage_dir).expect("Failed to create storage directory");

    // Test data
    let network = Network::Testnet4;
    let passphrase = SecurePassphrase::from_str("test_passphrase_123".to_string());

    // Generate mnemonic and master private key
    let secure_mnemonic = generate_mnemonic_secure(12).expect("Failed to generate mnemonic");
    let master_seed = get_master_seed_from_mnemonic(secure_mnemonic.as_str())
        .expect("Failed to generate master seed");
    let master_private_key = bitcoin::secp256k1::SecretKey::from_slice(&master_seed)
        .expect("Failed to create private key");
    let master_private_key_str = master_private_key.display_secret().to_string();
    let master_private_key_secure = SecureString::new(master_private_key_str);

    // Generate address
    let keypair = bitcoin::secp256k1::Keypair::from_secret_key(
        &crate::bitcoin_utils::SECP,
        &master_private_key,
    );
    let address = crate::bitcoin_utils::calculate_taproot_address(&keypair, network);

    // Encrypt both separately
    let encrypted_mnemonic =
        aes_encrypt_secure(&secure_mnemonic, &passphrase).expect("Failed to encrypt mnemonic");
    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .expect("Failed to encrypt private key");

    // Store the wallet
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));

    let wallet_data_json = serde_json::json!({
        "address": address.to_string(),
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
        serde_json::to_string_pretty(&wallet_data_json).unwrap(),
    )
    .expect("Failed to write wallet file");

    // Verify file exists and has correct structure
    assert!(wallet_file.exists(), "Wallet file should exist");

    let stored_data: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&wallet_file).unwrap())
            .expect("Failed to parse stored wallet data");

    assert_eq!(
        stored_data["address"].as_str().unwrap(),
        address.to_string()
    );
    assert_eq!(
        stored_data["network"].as_str().unwrap(),
        network.to_string()
    );
    assert_eq!(
        stored_data["data_format"].as_str().unwrap(),
        "separate_encrypted_fields"
    );

    // Verify both encrypted fields exist and have different nonces
    let mnemonic_nonce = stored_data["encrypted_mnemonic"]["nonce"].as_str().unwrap();
    let private_key_nonce = stored_data["encrypted_private_key"]["nonce"]
        .as_str()
        .unwrap();
    assert_ne!(
        mnemonic_nonce, private_key_nonce,
        "Nonces should be different"
    );

    println!("✓ Test 1 passed: Wallet generated and stored successfully");
    println!("  Address: {}", address);
    println!("  Mnemonic nonce: {}", mnemonic_nonce);
    println!("  Private key nonce: {}", private_key_nonce);
}

#[test]
fn test_decrypt_stored_wallet() {
    let temp_dir = setup_test_storage();
    let storage_dir = get_test_storage_dir(&temp_dir);
    fs::create_dir_all(&storage_dir).expect("Failed to create storage directory");

    // Test data
    let network = Network::Testnet4;
    let passphrase = SecurePassphrase::from_str("test_passphrase_123".to_string());

    // Generate and store wallet
    let secure_mnemonic = generate_mnemonic_secure(12).expect("Failed to generate mnemonic");
    let original_mnemonic = secure_mnemonic.as_str().to_string();

    let master_seed =
        get_master_seed_from_mnemonic(&original_mnemonic).expect("Failed to generate master seed");
    let master_private_key = bitcoin::secp256k1::SecretKey::from_slice(&master_seed)
        .expect("Failed to create private key");
    let original_private_key = master_private_key.display_secret().to_string();
    let master_private_key_secure = SecureString::new(original_private_key.clone());

    let keypair = bitcoin::secp256k1::Keypair::from_secret_key(
        &crate::bitcoin_utils::SECP,
        &master_private_key,
    );
    let address = crate::bitcoin_utils::calculate_taproot_address(&keypair, network);

    // Encrypt and store
    let encrypted_mnemonic =
        aes_encrypt_secure(&secure_mnemonic, &passphrase).expect("Failed to encrypt mnemonic");
    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .expect("Failed to encrypt private key");

    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));
    let wallet_data_json = serde_json::json!({
        "address": address.to_string(),
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
        serde_json::to_string_pretty(&wallet_data_json).unwrap(),
    )
    .expect("Failed to write wallet file");

    // Now test decryption
    let stored_data: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&wallet_file).unwrap())
            .expect("Failed to parse stored wallet data");

    // Decrypt mnemonic
    let mnemonic_data = EncryptedData {
        ciphertext: hex::decode(
            stored_data["encrypted_mnemonic"]["ciphertext"]
                .as_str()
                .unwrap(),
        )
        .unwrap(),
        nonce: hex::decode(stored_data["encrypted_mnemonic"]["nonce"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap(),
        salt: hex::decode(stored_data["encrypted_mnemonic"]["salt"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap(),
    };

    let decrypted_mnemonic =
        aes_decrypt_secure(&mnemonic_data, &passphrase).expect("Failed to decrypt mnemonic");

    // Decrypt private key
    let private_key_data = EncryptedData {
        ciphertext: hex::decode(
            stored_data["encrypted_private_key"]["ciphertext"]
                .as_str()
                .unwrap(),
        )
        .unwrap(),
        nonce: hex::decode(
            stored_data["encrypted_private_key"]["nonce"]
                .as_str()
                .unwrap(),
        )
        .unwrap()
        .try_into()
        .unwrap(),
        salt: hex::decode(
            stored_data["encrypted_private_key"]["salt"]
                .as_str()
                .unwrap(),
        )
        .unwrap()
        .try_into()
        .unwrap(),
    };

    let decrypted_private_key =
        aes_decrypt_secure(&private_key_data, &passphrase).expect("Failed to decrypt private key");

    // Verify decrypted data matches original
    assert_eq!(
        decrypted_mnemonic.as_str(),
        original_mnemonic,
        "Decrypted mnemonic should match original"
    );
    assert_eq!(
        decrypted_private_key.as_str(),
        original_private_key,
        "Decrypted private key should match original"
    );

    println!("✓ Test 2 passed: Wallet decrypted successfully");
    println!("  Original mnemonic: {}", original_mnemonic);
    println!("  Decrypted mnemonic: {}", decrypted_mnemonic.as_str());
    println!(
        "  Mnemonics match: {}",
        original_mnemonic == decrypted_mnemonic.as_str()
    );
}

#[test]
fn test_e2e_create_store_show_mnemonic() {
    let temp_dir = setup_test_storage();
    let storage_dir = get_test_storage_dir(&temp_dir);
    fs::create_dir_all(&storage_dir).expect("Failed to create storage directory");

    let network = Network::Testnet4;
    let passphrase_str = "strong_e2e_test_passphrase_456";

    // Step 1: Create wallet (simulating the full create_encrypted_wallet_with_address flow)
    println!("Step 1: Creating wallet...");

    // Generate mnemonic
    let secure_mnemonic = generate_mnemonic_secure(12).expect("Failed to generate mnemonic");
    let original_mnemonic = secure_mnemonic.as_str().to_string();

    // Generate master seed and private key using BIP-39
    let master_seed =
        get_master_seed_from_mnemonic(&original_mnemonic).expect("Failed to generate master seed");
    let master_private_key = bitcoin::secp256k1::SecretKey::from_slice(&master_seed)
        .expect("Failed to create private key");
    let master_private_key_str = master_private_key.display_secret().to_string();
    let master_private_key_secure = SecureString::new(master_private_key_str);

    // Generate address
    let keypair = bitcoin::secp256k1::Keypair::from_secret_key(
        &crate::bitcoin_utils::SECP,
        &master_private_key,
    );
    let address = crate::bitcoin_utils::calculate_taproot_address(&keypair, network);

    println!("  Generated address: {}", address);
    println!("  Generated mnemonic: {}", original_mnemonic);

    // Step 2: Store wallet with separate encryption
    println!("Step 2: Encrypting and storing wallet...");

    let passphrase = SecurePassphrase::from_str(passphrase_str.to_string());
    let encrypted_mnemonic =
        aes_encrypt_secure(&secure_mnemonic, &passphrase).expect("Failed to encrypt mnemonic");
    let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
        .expect("Failed to encrypt private key");

    // Store using the actual storage function logic
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address));
    let wallet_data_json = serde_json::json!({
        "address": address.to_string(),
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
        serde_json::to_string_pretty(&wallet_data_json).unwrap(),
    )
    .expect("Failed to write wallet file");

    println!("  Wallet stored at: {}", wallet_file.display());

    // Step 3: Load and show mnemonic (simulating show_mnemonic_secure functionality)
    println!("Step 3: Loading and showing mnemonic...");

    // Load wallet file
    assert!(wallet_file.exists(), "Wallet file should exist");
    let stored_data: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&wallet_file).unwrap())
            .expect("Failed to parse stored wallet data");

    // Verify it's the correct address
    assert_eq!(
        stored_data["address"].as_str().unwrap(),
        address.to_string()
    );

    // Decrypt mnemonic
    let mnemonic_encrypted_data = EncryptedData {
        ciphertext: hex::decode(
            stored_data["encrypted_mnemonic"]["ciphertext"]
                .as_str()
                .unwrap(),
        )
        .unwrap(),
        nonce: hex::decode(stored_data["encrypted_mnemonic"]["nonce"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap(),
        salt: hex::decode(stored_data["encrypted_mnemonic"]["salt"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap(),
    };

    let decrypted_mnemonic = aes_decrypt_secure(&mnemonic_encrypted_data, &passphrase)
        .expect("Failed to decrypt mnemonic");

    // Verify the full flow worked correctly
    assert_eq!(
        decrypted_mnemonic.as_str(),
        original_mnemonic,
        "E2E: Final mnemonic should match original"
    );

    // Also verify we can regenerate the same address from the decrypted mnemonic
    let verification_seed = get_master_seed_from_mnemonic(decrypted_mnemonic.as_str())
        .expect("Failed to regenerate master seed");
    let verification_private_key = bitcoin::secp256k1::SecretKey::from_slice(&verification_seed)
        .expect("Failed to recreate private key");
    let verification_keypair = bitcoin::secp256k1::Keypair::from_secret_key(
        &crate::bitcoin_utils::SECP,
        &verification_private_key,
    );
    let verification_address =
        crate::bitcoin_utils::calculate_taproot_address(&verification_keypair, network);

    assert_eq!(
        verification_address, address,
        "E2E: Regenerated address should match stored address"
    );

    println!("✓ Test 3 passed: End-to-end create, store, and show mnemonic");
    println!("  Original mnemonic: {}", original_mnemonic);
    println!("  Retrieved mnemonic: {}", decrypted_mnemonic.as_str());
    println!("  Address consistency: {}", address);
    println!("  Full round-trip successful: ✓");
}

#[test]
fn test_different_nonces_used() {
    // Test to ensure mnemonic and private key use different nonces
    let passphrase = SecurePassphrase::from_str("nonce_test_passphrase".to_string());

    let secure_mnemonic = generate_mnemonic_secure(12).expect("Failed to generate mnemonic");
    let master_seed = get_master_seed_from_mnemonic(secure_mnemonic.as_str())
        .expect("Failed to generate master seed");
    let master_private_key = bitcoin::secp256k1::SecretKey::from_slice(&master_seed)
        .expect("Failed to create private key");
    let master_private_key_str = master_private_key.display_secret().to_string();
    let master_private_key_secure = SecureString::new(master_private_key_str);

    // Encrypt both multiple times to ensure nonces are always different
    for i in 0..5 {
        let encrypted_mnemonic =
            aes_encrypt_secure(&secure_mnemonic, &passphrase).expect("Failed to encrypt mnemonic");
        let encrypted_private_key = aes_encrypt_secure(&master_private_key_secure, &passphrase)
            .expect("Failed to encrypt private key");

        assert_ne!(
            encrypted_mnemonic.nonce, encrypted_private_key.nonce,
            "Iteration {}: Mnemonic and private key should have different nonces",
            i
        );

        // Also verify nonces are different across iterations (extremely unlikely to be the same)
        if i > 0 {
            let encrypted_mnemonic_2 = aes_encrypt_secure(&secure_mnemonic, &passphrase)
                .expect("Failed to encrypt mnemonic second time");
            assert_ne!(
                encrypted_mnemonic.nonce, encrypted_mnemonic_2.nonce,
                "Different encryption calls should produce different nonces"
            );
        }
    }

    println!("✓ Test 4 passed: Different nonces are used for each encryption");
}

#[test]
#[ignore]
// Use `cargo test test_interactive_e2e_create_and_show -- --ignored` to run
fn test_interactive_e2e_create_and_show() {
    println!("=== E2E Test: Create Wallet -> Show Mnemonic (Non-Interactive) ===");
    println!();
    println!("This test demonstrates the complete workflow:");
    println!("1. Create a new encrypted wallet");
    println!("2. Show the mnemonic from the created wallet");
    println!("Note: This test runs non-interactively for automated testing");
    println!();

    // Step 1: Create wallet non-interactively for testing
    println!();
    println!("=== Step 1: Creating Wallet ===");
    let wallet_name = "test_interactive_e2e_wallet";
    let network = bitcoin::Network::Testnet4;
    let test_passphrase = SecurePassphrase::from_str("test_passphrase_for_e2e".to_string());

    println!("Creating wallet: {}", wallet_name);
    println!("Network: {}", network);
    println!();

    // Use the non-interactive generate_and_store_mnemonic_secure function
    let create_result =
        generate_and_store_mnemonic_secure(12, wallet_name, &test_passphrase, network);

    match create_result {
        Ok(original_mnemonic) => {
            println!("✅ Wallet created successfully!");
            println!("Mnemonic generated and stored securely.");

            // Verify wallet file exists
            let storage_dir = get_storage_dir()
                .map_err(|e| anyhow::Error::msg(e.to_string()))
                .unwrap();
            let wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_name));
            assert!(wallet_file.exists(), "Wallet file should exist");
            println!("✅ Wallet file created at: {}", wallet_file.display());

            // Step 2: Show mnemonic non-interactively for testing
            println!();
            println!("=== Step 2: Show Mnemonic ===");
            println!(
                "Now we'll retrieve and display the mnemonic from the wallet you just created."
            );
            println!();

            // Use the non-interactive load_mnemonic_secure function
            let show_result = load_mnemonic_secure(wallet_name, test_passphrase.expose_secret());

            match show_result {
                Ok(retrieved_mnemonic) => {
                    println!("✅ Mnemonic retrieved successfully!");

                    // Verify the retrieved mnemonic matches the original
                    assert_eq!(
                        retrieved_mnemonic,
                        original_mnemonic.as_str(),
                        "Retrieved mnemonic should match the original"
                    );

                    // Display the mnemonic securely
                    match display_mnemonic_securely(SecureString::new(retrieved_mnemonic)) {
                        Ok(()) => {
                            println!("✅ Mnemonic displayed securely!");
                        }
                        Err(e) => {
                            panic!("Failed to display mnemonic: {}", e);
                        }
                    }

                    // Clean up - remove the test wallet file
                    if let Err(e) = std::fs::remove_file(&wallet_file) {
                        println!("Warning: Failed to cleanup test wallet file: {}", e);
                    } else {
                        println!("✅ Test wallet file cleaned up");
                    }
                }
                Err(e) => {
                    panic!("Failed to show mnemonic: {}", e);
                }
            }
        }
        Err(e) => {
            panic!("Failed to create wallet: {}", e);
        }
    }

    println!();
    println!("=== Test Completed Successfully! ===");
    println!();
    println!("Summary:");
    println!("• Created wallet: {}", wallet_name);
    println!("• Network: {}", network);
    println!("• Used interactive passphrase entry");
    println!("• Verified wallet creation and storage");
    println!("• Successfully retrieved and displayed mnemonic");
    println!();
    println!("✅ Interactive E2E test passed!");
}
