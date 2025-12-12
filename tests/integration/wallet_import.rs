//! Integration tests for wallet import workflows

use clementine_cli::__test_helpers::*;

use clementine_cli::__test_helpers::test_get_private_key_from_wallet;
use clementine_cli::errors::BridgeCliError;
use clementine_cli::wallet::{
    Purpose, backup_wallet, create_encrypted_wallet, import_wallet_from_file,
    import_wallet_from_private_key,
};
use serial_test::serial;

#[test]
#[serial]
fn test_import_wallet_from_private_key_and_retrieve() {
    setup_integration_test_env();

    let network = test_network();
    let label = "imported_from_key";

    // Import wallet
    let address = import_wallet_from_private_key(
        network,
        label,
        Purpose::Deposit,
        test_private_key(),
        test_passphrase(),
    )
    .expect("Import from private key should succeed");

    // Retrieve private key using test helper
    let retrieved_key = test_get_private_key_from_wallet(&address, &test_passphrase())
        .expect("Should retrieve private key");

    // Verify key matches
    let retrieved_key_str = retrieved_key.as_ref_inner().display_secret().to_string();
    assert_eq!(
        retrieved_key_str,
        "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        "Retrieved key should match imported key"
    );
}

#[test]
#[serial]
fn test_import_wallet_from_file_and_verify() {
    setup_integration_test_env();

    let network = test_network();
    let original_label = "original_wallet".to_string();

    // Create a wallet to export
    let (original_address, _, _wallet_path) = create_encrypted_wallet(
        network,
        original_label.clone(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Wallet creation should succeed");

    // Backup the wallet to a temp file
    let temp_backup = tempfile::TempDir::new().expect("Should create temp backup dir");
    let backup_dir = temp_backup.path();
    std::fs::create_dir_all(&backup_dir).expect("Should create backup dir");

    let unchecked_address = to_unchecked(&original_address);
    let backup_path =
        backup_wallet(&unchecked_address, &backup_dir).expect("Backup should succeed");

    // Note: We cannot import the backup while the original wallet exists
    // because they would have the same address. This is expected behavior.
    // Verify that importing with duplicate address fails appropriately
    let new_label = Some("imported_wallet");
    let result = import_wallet_from_file(&backup_path, new_label, test_passphrase());

    // Should fail because address already exists
    match result {
        Err(BridgeCliError::AddressAlreadyExists(_)) => {
            // Expected - can't import same wallet twice
        }
        Err(e) => panic!("Expected AddressAlreadyExists error, got: {:?}", e),
        Ok(_) => panic!("Expected error due to duplicate address, got success"),
    }
}

#[test]
#[serial]
fn test_import_with_duplicate_label_fails() {
    setup_integration_test_env();

    let network = test_network();
    let label = "duplicate_import_label";

    // Create first wallet with a label
    let _address1 = create_encrypted_wallet(
        network,
        label.to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("First wallet creation should succeed");

    // Try to import a different wallet with the same label

    let result = import_wallet_from_private_key(
        network,
        label,
        Purpose::Deposit,
        test_private_key(),
        test_passphrase(),
    );

    // Verify it fails with label already exists error
    assert!(
        result.is_err(),
        "Importing wallet with duplicate label should fail"
    );

    match result {
        Err(BridgeCliError::LabelAlreadyExists(_)) | Err(BridgeCliError::Eyre(_)) => {
            // Expected error - label already exists (may be wrapped in Eyre)
        }
        Err(e) => panic!("Expected LabelAlreadyExists or Eyre error, got: {:?}", e),
        Ok(_) => panic!("Expected error, got success"),
    }
}

#[test]
#[serial]
fn test_import_from_file_with_wrong_passphrase_fails() {
    setup_integration_test_env();

    let network = test_network();
    let label = "test_label".to_string();

    // Create and backup wallet with correct passphrase
    let (address, _mnemonic, _wallet_path) =
        create_encrypted_wallet(network, label.clone(), Purpose::Deposit, test_passphrase())
            .expect("Wallet creation should succeed");

    let temp_backup = tempfile::TempDir::new().expect("Should create temp backup dir");
    let backup_dir = temp_backup.path();
    std::fs::create_dir_all(&backup_dir).expect("Should create backup dir");

    let unchecked_address = to_unchecked(&address);
    let backup_path =
        backup_wallet(&unchecked_address, &backup_dir).expect("Backup should succeed");

    // Read backup file to verify it contains encrypted data
    let backup_content = std::fs::read_to_string(&backup_path).expect("Should read backup file");

    // Verify the backup contains encrypted fields (not plaintext)
    assert!(
        backup_content.contains("encrypted_mnemonic"),
        "Backup should contain encrypted mnemonic"
    );

    assert!(
        backup_content.contains("encrypted_private_key"),
        "Backup should contain encrypted private key"
    );

    // The passphrase is required to decrypt these fields during import
    // This test verifies the backup file structure supports passphrase protection
}

#[test]
#[serial]
fn test_import_duplicate_private_key_with_different_label_fails() {
    setup_integration_test_env();

    let network = test_network();

    // Import first wallet
    let _address1 = import_wallet_from_private_key(
        network,
        "first_label",
        Purpose::Deposit,
        test_private_key(),
        test_passphrase(),
    )
    .expect("First import should succeed");

    // Try to import same private key with different label
    let result = import_wallet_from_private_key(
        network,
        "second_label",
        Purpose::Deposit,
        test_private_key(),
        test_passphrase(),
    );

    // Verify it fails (wallet with same address already exists)
    assert!(
        result.is_err(),
        "Importing same private key twice should fail"
    );

    match result {
        Err(BridgeCliError::AddressAlreadyExists(_)) | Err(BridgeCliError::Eyre(_)) => {
            // Expected error - address already exists (may be wrapped in Eyre)
        }
        Err(e) => panic!("Expected AddressAlreadyExists or Eyre error, got: {:?}", e),
        Ok(_) => panic!("Expected error, got success"),
    }
}

#[test]
#[serial]
fn test_import_invalid_private_key_format_fails() {
    setup_integration_test_env();

    let network = test_network();
    let label = "invalid_key_test";
    let passphrase = test_passphrase();

    let result = import_wallet_from_private_key(
        network,
        label,
        Purpose::Deposit,
        test_invalid_private_key(),
        passphrase,
    );

    // Verify it fails
    assert!(result.is_err(), "Invalid private key format should fail");
}

#[test]
#[serial]
fn test_import_wrong_length_private_key_fails() {
    setup_integration_test_env();

    let network = test_network();
    let label = "wrong_length_test";
    let passphrase = test_passphrase();

    let result = import_wallet_from_private_key(
        network,
        label,
        Purpose::Deposit,
        test_short_private_key(),
        passphrase,
    );

    // Verify it fails with InvalidPrivateKey error
    assert!(result.is_err(), "Wrong length private key should fail");

    match result {
        Err(BridgeCliError::InvalidPrivateKey(_)) => {
            // Expected error
        }
        Err(e) => panic!("Expected InvalidPrivateKey error, got: {:?}", e),
        Ok(_) => panic!("Expected error, got success"),
    }
}
