mod tests {
    use anyhow::anyhow;
    use bitcoin::Network;
    use clementine_cli::{
        address::{extract_address_from_wallet, generate_address_from_mnemonic_secure},
        bitcoin_utils::{self},
        encryption::{EncryptedData, aes_decrypt_secure, aes_encrypt_secure},
        mnemonic::{generate_mnemonic_secure, get_master_seed_from_mnemonic},
        secure_structs::SecureString,
    };
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

    pub fn load_wallet_from_file(file: &str) -> Result<serde_json::Value, anyhow::Error> {
        if !std::path::Path::new(file).exists() {
            return Err(anyhow!("Wallet file not found: {}", file));
        }

        let file_content = std::fs::read_to_string(file)?;
        let wallet_data: serde_json::Value = serde_json::from_str(&file_content)?;

        Ok(wallet_data)
    }

    #[test]
    fn test_generate_and_store_wallet() {
        let temp_dir = setup_test_storage();
        let storage_dir = get_test_storage_dir(&temp_dir);

        // Create storage directory
        fs::create_dir_all(&storage_dir).expect("Failed to create storage directory");

        // Test data
        let network = Network::Testnet4;
        let passphrase = SecureString::init_with(|| "test_passphrase_123".to_string());

        // Generate mnemonic and master private key
        let secure_mnemonic = generate_mnemonic_secure().expect("Failed to generate mnemonic");
        let master_seed = get_master_seed_from_mnemonic(&secure_mnemonic)
            .expect("Failed to generate master seed");
        let master_private_key = bitcoin::secp256k1::SecretKey::from_slice(&master_seed)
            .expect("Failed to create private key");
        let master_private_key_str = master_private_key.display_secret().to_string();
        let master_private_key_secure = SecureString::init_with(|| master_private_key_str);

        // Generate address
        let keypair =
            bitcoin::secp256k1::Keypair::from_secret_key(&bitcoin_utils::SECP, &master_private_key);
        let address = bitcoin_utils::calculate_taproot_address(&keypair, network);

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
                "nonce": hex::encode(encrypted_mnemonic.nonce),
                "salt": hex::encode(encrypted_mnemonic.salt)
            },
            "encrypted_private_key": {
                "ciphertext": hex::encode(&encrypted_private_key.ciphertext),
                "nonce": hex::encode(encrypted_private_key.nonce),
                "salt": hex::encode(encrypted_private_key.salt)
            },
            "created_at": chrono::Utc::now().to_rfc3339(),
            "encryption_method": "aes256_gcm_argon2id_secure",
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
        let passphrase = SecureString::init_with(|| "test_passphrase_123".to_string());

        // Generate and store wallet
        let secure_mnemonic = generate_mnemonic_secure().expect("Failed to generate mnemonic");
        let original_mnemonic = secure_mnemonic.expose_secret().to_string();

        let master_seed = get_master_seed_from_mnemonic(&secure_mnemonic)
            .expect("Failed to generate master seed");
        let master_private_key = bitcoin::secp256k1::SecretKey::from_slice(&master_seed)
            .expect("Failed to create private key");
        let original_private_key = master_private_key.display_secret().to_string();
        let master_private_key_secure = SecureString::init_with(|| original_private_key.clone());

        let keypair =
            bitcoin::secp256k1::Keypair::from_secret_key(&bitcoin_utils::SECP, &master_private_key);
        let address = bitcoin_utils::calculate_taproot_address(&keypair, network);

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
                "nonce": hex::encode(encrypted_mnemonic.nonce),
                "salt": hex::encode(encrypted_mnemonic.salt)
            },
            "encrypted_private_key": {
                "ciphertext": hex::encode(&encrypted_private_key.ciphertext),
                "nonce": hex::encode(encrypted_private_key.nonce),
                "salt": hex::encode(encrypted_private_key.salt)
            },
            "created_at": chrono::Utc::now().to_rfc3339(),
            "encryption_method": "aes256_gcm_argon2id_secure",
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

        let decrypted_private_key = aes_decrypt_secure(&private_key_data, &passphrase)
            .expect("Failed to decrypt private key");

        // Verify decrypted data matches original
        assert_eq!(
            decrypted_mnemonic.expose_secret().to_string(),
            original_mnemonic,
            "Decrypted mnemonic should match original"
        );
        assert_eq!(
            decrypted_private_key.expose_secret().to_string(),
            original_private_key,
            "Decrypted private key should match original"
        );

        println!("✓ Test 2 passed: Wallet decrypted successfully");
        println!("  Original mnemonic: {}", original_mnemonic);
        println!(
            "  Decrypted mnemonic: {}",
            decrypted_mnemonic.expose_secret()
        );
        println!(
            "  Mnemonics match: {}",
            original_mnemonic == *decrypted_mnemonic.expose_secret()
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
        let secure_mnemonic = generate_mnemonic_secure().expect("Failed to generate mnemonic");
        let original_mnemonic = secure_mnemonic.expose_secret().to_string();

        // Generate master seed and private key using BIP-39
        let master_seed = get_master_seed_from_mnemonic(&secure_mnemonic)
            .expect("Failed to generate master seed");
        let master_private_key = bitcoin::secp256k1::SecretKey::from_slice(&master_seed)
            .expect("Failed to create private key");
        let master_private_key_str = master_private_key.display_secret().to_string();
        let master_private_key_secure = SecureString::init_with(|| master_private_key_str);

        // Generate address
        let keypair =
            bitcoin::secp256k1::Keypair::from_secret_key(&bitcoin_utils::SECP, &master_private_key);
        let address = bitcoin_utils::calculate_taproot_address(&keypair, network);

        println!("  Generated address: {}", address);
        println!("  Generated mnemonic: {}", original_mnemonic);

        // Step 2: Store wallet with separate encryption
        println!("Step 2: Encrypting and storing wallet...");

        let passphrase = SecureString::init_with(|| passphrase_str.to_string());
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
                "nonce": hex::encode(encrypted_mnemonic.nonce),
                "salt": hex::encode(encrypted_mnemonic.salt)
            },
            "encrypted_private_key": {
                "ciphertext": hex::encode(&encrypted_private_key.ciphertext),
                "nonce": hex::encode(encrypted_private_key.nonce),
                "salt": hex::encode(encrypted_private_key.salt)
            },
            "created_at": chrono::Utc::now().to_rfc3339(),
            "encryption_method": "aes256_gcm_argon2id_secure",
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
            decrypted_mnemonic.expose_secret().to_string(),
            original_mnemonic,
            "E2E: Final mnemonic should match original"
        );

        // Also verify we can regenerate the same address from the decrypted mnemonic
        let verification_seed = get_master_seed_from_mnemonic(&decrypted_mnemonic)
            .expect("Failed to regenerate master seed");
        let verification_private_key =
            bitcoin::secp256k1::SecretKey::from_slice(&verification_seed)
                .expect("Failed to recreate private key");
        let verification_keypair = bitcoin::secp256k1::Keypair::from_secret_key(
            &bitcoin_utils::SECP,
            &verification_private_key,
        );
        let verification_address =
            bitcoin_utils::calculate_taproot_address(&verification_keypair, network);

        assert_eq!(
            verification_address, address,
            "E2E: Regenerated address should match stored address"
        );

        println!("✓ Test 3 passed: End-to-end create, store, and show mnemonic");
        println!("  Original mnemonic: {}", original_mnemonic);
        println!(
            "  Retrieved mnemonic: {}",
            decrypted_mnemonic.expose_secret()
        );
        println!("  Address consistency: {}", address);
        println!("  Full round-trip successful: ✓");
    }

    #[test]
    fn test_different_nonces_used() {
        // Test to ensure mnemonic and private key use different nonces
        let passphrase = SecureString::init_with(|| "nonce_test_passphrase".to_string());

        let secure_mnemonic = generate_mnemonic_secure().expect("Failed to generate mnemonic");
        let master_seed = get_master_seed_from_mnemonic(&secure_mnemonic)
            .expect("Failed to generate master seed");
        let master_private_key = bitcoin::secp256k1::SecretKey::from_slice(&master_seed)
            .expect("Failed to create private key");
        let master_private_key_str = master_private_key.display_secret().to_string();
        let master_private_key_secure = SecureString::init_with(|| master_private_key_str);

        // Encrypt both multiple times to ensure nonces are always different
        for i in 0..5 {
            let encrypted_mnemonic = aes_encrypt_secure(&secure_mnemonic, &passphrase)
                .expect("Failed to encrypt mnemonic");
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

    // ============================================================================
    // Tests for ImportWithMnemonic functionality
    // ============================================================================

    /// Helper function to create a test wallet file with known mnemonic and address
    fn create_test_wallet_file(
        temp_dir: &tempfile::TempDir,
        mnemonic: &SecureString,
        network: Network,
    ) -> (std::path::PathBuf, String) {
        let storage_dir = get_test_storage_dir(temp_dir);
        fs::create_dir_all(&storage_dir).expect("Failed to create storage directory");

        // Generate address from the known mnemonic
        let master_seed =
            get_master_seed_from_mnemonic(mnemonic).expect("Failed to generate master seed");
        let master_private_key = bitcoin::secp256k1::SecretKey::from_slice(&master_seed)
            .expect("Failed to create private key");
        let keypair =
            bitcoin::secp256k1::Keypair::from_secret_key(&bitcoin_utils::SECP, &master_private_key);
        let address = bitcoin_utils::calculate_taproot_address(&keypair, network);

        // Create wallet file with the address
        let wallet_file = storage_dir.join(format!("wallet_{}.json", address));
        let wallet_data_json = serde_json::json!({
            "address": address.to_string(),
            "network": network.to_string(),
            "created_at": chrono::Utc::now().to_rfc3339(),
            "encryption_method": "test_wallet",
            "data_format": "test_format"
        });

        fs::write(
            &wallet_file,
            serde_json::to_string_pretty(&wallet_data_json).unwrap(),
        )
        .expect("Failed to write test wallet file");

        (wallet_file, address.to_string())
    }

    /// Test successful import with valid mnemonic
    #[test]
    fn test_import_with_mnemonic_success() {
        let temp_dir = setup_test_storage();
        let network = Network::Testnet4;

        // Use a known valid BIP-39 mnemonic for testing
        let test_mnemonic = SecureString::init_with(|| {
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about".to_string()
        });

        // Create test wallet file with the address that matches this mnemonic
        let (wallet_file, expected_address) =
            create_test_wallet_file(&temp_dir, &test_mnemonic, network);

        println!("Test wallet created:");
        println!("  File: {}", wallet_file.display());
        println!("  Expected address: {}", expected_address);
        println!("  Test mnemonic: {}", test_mnemonic.expose_secret());

        // Test the helper functions directly first
        println!("\nTesting helper functions:");

        // Test load_wallet_from_file
        let wallet_data = load_wallet_from_file(wallet_file.to_str().unwrap())
            .expect("Should load wallet file successfully");
        println!("✅ Wallet file loaded successfully");

        // Test extract_address_from_wallet
        let extracted_address =
            extract_address_from_wallet(&wallet_data).expect("Should extract address successfully");
        assert_eq!(
            extracted_address, expected_address,
            "Extracted address should match expected"
        );
        println!("✅ Address extracted successfully: {}", extracted_address);

        let generated_address = generate_address_from_mnemonic_secure(&test_mnemonic, network)
            .expect("Should generate address from mnemonic");
        assert_eq!(
            generated_address, expected_address,
            "Generated address should match expected"
        );
        println!(
            "✅ Address generated from mnemonic successfully: {}",
            generated_address
        );

        println!();
        println!("✅ Test passed: ImportWithMnemonic helper functions work correctly");
        println!("  Wallet file: {}", wallet_file.display());
        println!("  Address consistency verified: {}", expected_address);
    }

    /// Test failed import with invalid mnemonic (wrong address)
    #[test]
    fn test_import_with_mnemonic_failure_wrong_mnemonic() {
        let temp_dir = setup_test_storage();
        let network = Network::Testnet4;

        // Create wallet with one mnemonic
        let correct_mnemonic = SecureString::init_with(|| {
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about".to_string()
        });
        let (wallet_file, expected_address) =
            create_test_wallet_file(&temp_dir, &correct_mnemonic, network);

        // Try to verify with a different valid mnemonic (24 words)
        let wrong_mnemonic = SecureString::init_with(|| {
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art".to_string()
        });

        println!("Testing with wrong mnemonic:");
        println!("  Correct mnemonic: {}", correct_mnemonic.expose_secret());
        println!("  Wrong mnemonic: {}", wrong_mnemonic.expose_secret());
        println!("  Expected address: {}", expected_address);

        // Load wallet data
        let wallet_data = load_wallet_from_file(wallet_file.to_str().unwrap())
            .expect("Should load wallet file successfully");

        // Extract address
        let extracted_address =
            extract_address_from_wallet(&wallet_data).expect("Should extract address successfully");

        // Generate address from wrong mnemonic
        let generated_address = generate_address_from_mnemonic_secure(&wrong_mnemonic, network)
            .expect("Should generate address from wrong mnemonic");

        println!("  Extracted address: {}", extracted_address);
        println!("  Generated address: {}", generated_address);

        // Addresses should NOT match
        assert_ne!(
            extracted_address, generated_address,
            "Addresses should not match when using wrong mnemonic"
        );

        println!();
        println!("✅ Test passed: ImportWithMnemonic correctly fails with wrong mnemonic");
        println!("  Wallet address: {}", extracted_address);
        println!("  Generated address: {}", generated_address);
        println!("  Addresses correctly don't match ✓");
    }

    /// Test with invalid wallet file (missing address field)
    #[test]
    fn test_import_with_mnemonic_invalid_wallet_file() {
        let temp_dir = setup_test_storage();
        let storage_dir = get_test_storage_dir(&temp_dir);
        fs::create_dir_all(&storage_dir).expect("Failed to create storage directory");

        // Create invalid wallet file without address field
        let invalid_wallet_file = storage_dir.join("invalid_wallet.json");
        let invalid_wallet_data = serde_json::json!({
            "network": "testnet4",
            "created_at": chrono::Utc::now().to_rfc3339(),
            // Missing "address" field
        });

        fs::write(
            &invalid_wallet_file,
            serde_json::to_string_pretty(&invalid_wallet_data).unwrap(),
        )
        .expect("Failed to write invalid wallet file");

        println!("Testing with invalid wallet file:");
        println!("  File: {}", invalid_wallet_file.display());

        // Test load_wallet_from_file (should succeed)
        let wallet_data = load_wallet_from_file(invalid_wallet_file.to_str().unwrap())
            .expect("Should load wallet file successfully even if invalid format");

        // Test extract_address_from_wallet (should fail)
        let result = extract_address_from_wallet(&wallet_data);
        assert!(
            result.is_err(),
            "Should fail to extract address from invalid wallet"
        );

        match result {
            Err(e) => {
                println!("✅ Correctly failed with error: {}", e);
                assert!(
                    e.to_string().contains("Address field not found"),
                    "Error should mention missing address field"
                );
            }
            Ok(_) => panic!("Should have failed to extract address"),
        }

        println!();
        println!("✅ Test passed: ImportWithMnemonic correctly handles invalid wallet files");
    }

    /// Test with non-existent wallet file
    #[test]
    fn test_import_with_mnemonic_nonexistent_file() {
        let temp_dir = setup_test_storage();
        let storage_dir = get_test_storage_dir(&temp_dir);
        let nonexistent_file = storage_dir.join("does_not_exist.json");

        println!("Testing with non-existent wallet file:");
        println!("  File: {}", nonexistent_file.display());

        // Test load_wallet_from_file (should fail)
        let result = load_wallet_from_file(nonexistent_file.to_str().unwrap());
        assert!(
            result.is_err(),
            "Should fail to load non-existent wallet file"
        );

        match result {
            Err(e) => {
                println!("✅ Correctly failed with error: {}", e);
                assert!(
                    e.to_string().contains("Wallet file not found"),
                    "Error should mention file not found"
                );
            }
            Ok(_) => panic!("Should have failed to load non-existent file"),
        }

        println!();
        println!("✅ Test passed: ImportWithMnemonic correctly handles non-existent files");
    }

    /// Test mnemonic validation in prompt_mnemonic_secure
    #[test]
    fn test_mnemonic_validation() {
        use bip39::{Language, Mnemonic};

        println!("Testing BIP-39 mnemonic validation:");

        // Test valid mnemonics
        let valid_mnemonics = [
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about", // 12 words
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon agent", // 18 words
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art", // 24 words
        ];

        for (i, mnemonic) in valid_mnemonics.iter().enumerate() {
            println!(
                "  Testing valid mnemonic {}: {} words",
                i + 1,
                mnemonic.split_whitespace().count()
            );
            let result = Mnemonic::parse(*mnemonic);
            assert!(
                result.is_ok(),
                "Valid mnemonic should parse successfully: {}",
                mnemonic
            );
            println!("    ✅ Parsed successfully");
        }

        // Test invalid mnemonics
        let invalid_mnemonics = [
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon invalid", // invalid word
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon", // 11 words (invalid length)
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon", // 13 words (invalid length)
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon invalid", // invalid word in 24-word mnemonic
            "", // empty
            "notaword notaword notaword notaword notaword notaword notaword notaword notaword notaword notaword notaword", // all invalid words
        ];

        for (i, mnemonic) in invalid_mnemonics.iter().enumerate() {
            println!(
                "  Testing invalid mnemonic {}: {} words",
                i + 1,
                mnemonic.split_whitespace().count()
            );
            let result = Mnemonic::parse(*mnemonic);
            assert!(
                result.is_err(),
                "Invalid mnemonic should fail to parse: {}",
                mnemonic
            );
            println!("    ✅ Correctly failed to parse");
        }

        // Test individual word validation (same logic as used in prompt_mnemonic_secure)
        let wordlist = Language::English.word_list();

        println!("  Testing individual word validation:");

        // Valid words
        let valid_words = vec!["abandon", "ability", "about", "above", "absent"];
        for word in &valid_words {
            let is_valid = wordlist.contains(word);
            assert!(is_valid, "Word '{}' should be valid", word);
            println!("    ✅ '{}' is valid", word);
        }

        // Invalid words
        let invalid_words = vec!["notaword", "invalid", "test123", "abandon123", ""];
        for word in &invalid_words {
            let is_valid = wordlist.contains(word);
            assert!(!is_valid, "Word '{}' should be invalid", word);
            println!("    ✅ '{}' is correctly invalid", word);
        }

        println!();
        println!("✅ Test passed: BIP-39 mnemonic validation works correctly");
    }
}
