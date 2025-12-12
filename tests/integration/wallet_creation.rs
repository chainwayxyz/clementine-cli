//! Integration tests for wallet creation workflows

use clementine_cli::__test_helpers::*;

use clementine_cli::__test_helpers::test_get_private_key_from_wallet;
use clementine_cli::errors::BridgeCliError;
use clementine_cli::wallet::{
    Purpose, create_encrypted_wallet, get_mnemonic_from_wallet, get_registry_wallet_set,
};
use serial_test::serial;

#[test]
#[serial]
fn test_create_wallet_and_retrieve_mnemonic() {
    setup_integration_test_env();

    let network = test_network();
    let label = "test_wallet".to_string();

    // Create wallet
    let (address, original_mnemonic, wallet_path) =
        create_encrypted_wallet(network, label.clone(), Purpose::Deposit, test_passphrase())
            .expect("Wallet creation should succeed");

    // Verify file exists
    assert!(wallet_path.exists(), "Wallet file should exist");

    // Retrieve mnemonic
    let retrieved_mnemonic =
        get_mnemonic_from_wallet(&address, &test_passphrase()).expect("Should retrieve mnemonic");

    // Verify match
    assert_eq!(
        original_mnemonic.to_string(),
        retrieved_mnemonic.to_string(),
        "Retrieved mnemonic should match original"
    );
}

#[test]
#[serial]
fn test_create_wallet_and_retrieve_private_key() {
    setup_integration_test_env();

    let network = test_network();
    let label = "test_wallet_key".to_string();

    // Create wallet
    let (address, _mnemonic, _wallet_path) = create_encrypted_wallet(
        network,
        label.clone(),
        Purpose::Withdrawal,
        test_passphrase(),
    )
    .expect("Wallet creation should succeed");

    // Retrieve private key first time
    let private_key_1 = test_get_private_key_from_wallet(&address, &test_passphrase())
        .expect("Should retrieve private key");

    // Retrieve private key second time
    let private_key_2 = test_get_private_key_from_wallet(&address, &test_passphrase())
        .expect("Should retrieve private key again");

    // Verify consistency (using display format since we can't expose secret directly)
    let key1_str = private_key_1.as_ref_inner().display_secret().to_string();
    let key2_str = private_key_2.as_ref_inner().display_secret().to_string();

    assert_eq!(
        key1_str, key2_str,
        "Private key retrieval should be deterministic"
    );

    // Verify key format (should be 64 hex characters)
    assert_eq!(
        key1_str.len(),
        64,
        "Private key should be 64 hex characters"
    );
}

#[test]
#[serial]
fn test_create_wallet_registers_in_registry() {
    setup_integration_test_env();

    let network = test_network();
    let label = "registry_test_wallet".to_string();

    // Create wallet
    let (address, _mnemonic, _wallet_path) =
        create_encrypted_wallet(network, label.clone(), Purpose::Deposit, test_passphrase())
            .expect("Wallet creation should succeed");

    // Get registry wallets
    let registry_wallets = get_registry_wallet_set().expect("Should retrieve registry");

    // Verify wallet appears in registry
    let unchecked_address = to_unchecked(&address);
    assert!(
        registry_wallets.contains(&unchecked_address),
        "Created wallet should appear in registry"
    );
}

#[test]
#[serial]
fn test_create_wallet_with_duplicate_label_fails() {
    setup_integration_test_env();

    let network = test_network();
    let label = "duplicate_label".to_string();

    // Create first wallet
    let _result1 =
        create_encrypted_wallet(network, label.clone(), Purpose::Deposit, test_passphrase())
            .expect("First wallet creation should succeed");

    // Attempt to create second wallet with same label
    let result2 =
        create_encrypted_wallet(network, label.clone(), Purpose::Deposit, test_passphrase());

    // Verify it fails
    assert!(
        result2.is_err(),
        "Creating wallet with duplicate label should fail"
    );

    // Verify it's the correct error type
    match result2 {
        Err(BridgeCliError::LabelAlreadyExists(_)) => {
            // Expected error
        }
        Err(e) => panic!("Expected LabelAlreadyExists error, got: {:?}", e),
        Ok(_) => panic!("Expected error, got success"),
    }
}

#[test]
#[serial]
#[cfg(unix)]
fn test_create_wallet_file_has_secure_permissions() {
    use std::os::unix::fs::PermissionsExt;

    setup_integration_test_env();

    let network = test_network();
    let label = "permissions_test".to_string();

    // Create wallet
    let (_address, _mnemonic, wallet_path) =
        create_encrypted_wallet(network, label.clone(), Purpose::Deposit, test_passphrase())
            .expect("Wallet creation should succeed");

    // Check file permissions
    let metadata = std::fs::metadata(&wallet_path).expect("Should read wallet file metadata");

    let permissions = metadata.permissions();
    let mode = permissions.mode();

    // Verify permissions are 0600 (owner read/write only)
    // The mode includes file type bits, so we mask with 0o777
    assert_eq!(
        mode & 0o777,
        0o600,
        "Wallet file should have 0600 permissions"
    );
}

#[test]
#[serial]
fn test_retrieve_mnemonic_with_wrong_passphrase_fails() {
    setup_integration_test_env();

    let network = test_network();
    let label = "wrong_pass_test".to_string();

    // Create wallet
    let (address, _mnemonic, _wallet_path) =
        create_encrypted_wallet(network, label.clone(), Purpose::Deposit, test_passphrase())
            .expect("Wallet creation should succeed");

    // Attempt to retrieve mnemonic with wrong passphrase
    let wrong_pass = wrong_passphrase();
    let result = get_mnemonic_from_wallet(&address, &wrong_pass);

    // Verify it fails
    assert!(
        result.is_err(),
        "Retrieving mnemonic with wrong passphrase should fail"
    );

    // Verify it's the correct error type
    match result {
        Err(BridgeCliError::DecryptionError) | Err(BridgeCliError::IncorrectPassphrase) => {
            // Expected error - wrong passphrase causes decryption failure
        }
        Err(e) => panic!(
            "Expected DecryptionError or IncorrectPassphrase, got: {:?}",
            e
        ),
        Ok(_) => panic!("Expected error, got success"),
    }
}
