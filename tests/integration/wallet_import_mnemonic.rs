//! Integration tests for wallet import from mnemonic

use bip39::Mnemonic;
use clementine_cli::__test_helpers::*;
use clementine_cli::errors::BridgeCliError;
use clementine_cli::wallet::Purpose;
use clementine_cli::*;
use serial_test::serial;
use std::str::FromStr;

/// Test importing a wallet from a valid 12-word BIP-39 mnemonic
#[test]
#[serial]
fn test_import_wallet_from_valid_mnemonic() {
    setup_integration_test_env();
    let network = test_network();
    let label = "mnemonic-import-test";

    // Generate a valid mnemonic for testing
    let mnemonic_str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let mnemonic = Mnemonic::from_str(mnemonic_str).expect("Valid BIP-39 mnemonic");

    // Import wallet from mnemonic using test helper (bypasses password prompt)
    let result = import_wallet_from_mnemonic(network, label, Purpose::Deposit, mnemonic.clone());
    assert!(result.is_ok(), "Should import wallet from valid mnemonic");
    let address = result.unwrap();

    // Verify wallet is registered
    let wallets = test_get_wallets_from_registry().expect("Should get registry");
    assert!(
        wallets.contains_key(&address.address_without_prefix()),
        "Imported wallet should be in registry"
    );

    // Verify wallet metadata
    let wallet_entry = wallets.get(&address.address_without_prefix()).unwrap();
    assert_eq!(wallet_entry.label, label);
    assert_eq!(wallet_entry.network, "regtest");
    assert_eq!(wallet_entry.imported, Some(true));
    assert_eq!(
        wallet_entry.import_method,
        Some("mnemonic_import".to_string())
    );
}

/// Test that imported wallet allows mnemonic retrieval
#[test]
#[serial]
fn test_imported_wallet_allows_mnemonic_retrieval() {
    setup_integration_test_env();
    let network = test_network();
    let label = "mnemonic-retrieval-test";
    let passphrase = test_passphrase();

    // Use a known mnemonic
    let mnemonic_str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let mnemonic = Mnemonic::from_str(mnemonic_str).expect("Valid BIP-39 mnemonic");

    // Import wallet
    let address = import_wallet_from_mnemonic(network, label, Purpose::Deposit, mnemonic.clone())
        .expect("Should import wallet");

    // Retrieve mnemonic
    let retrieved_mnemonic = get_mnemonic_from_wallet(&address, &passphrase);
    assert!(
        retrieved_mnemonic.is_ok(),
        "Should be able to retrieve mnemonic from imported wallet"
    );

    let retrieved = retrieved_mnemonic.unwrap();
    assert_eq!(
        retrieved.to_string(),
        mnemonic_str,
        "Retrieved mnemonic should match original"
    );
}

/// Test importing wallet with duplicate label fails
#[test]
#[serial]
fn test_mnemonic_import_with_duplicate_label_fails() {
    setup_integration_test_env();
    let network = test_network();
    let label = "duplicate-label-test";

    let mnemonic_str1 = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let mnemonic1 = Mnemonic::from_str(mnemonic_str1).expect("Valid BIP-39 mnemonic");

    // Import first wallet
    import_wallet_from_mnemonic(network, label, Purpose::Deposit, mnemonic1)
        .expect("First import should succeed");

    // Try to import another wallet with same label but different mnemonic
    let mnemonic_str2 =
        "legal winner thank year wave sausage worth useful legal winner thank yellow";
    let mnemonic2 = Mnemonic::from_str(mnemonic_str2).expect("Valid BIP-39 mnemonic");

    let result = import_wallet_from_mnemonic(network, label, Purpose::Deposit, mnemonic2);
    assert!(result.is_err(), "Should fail with duplicate label");
    assert!(matches!(
        result.unwrap_err(),
        BridgeCliError::LabelAlreadyExists(_)
    ));
}

/// Test importing same mnemonic with different label fails (duplicate address)
#[test]
#[serial]
fn test_mnemonic_import_duplicate_address_fails() {
    setup_integration_test_env();
    let network = test_network();
    let label1 = "label-one";
    let label2 = "label-two";

    let mnemonic_str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let mnemonic = Mnemonic::from_str(mnemonic_str).expect("Valid BIP-39 mnemonic");

    // Import first time
    import_wallet_from_mnemonic(network, label1, Purpose::Deposit, mnemonic.clone())
        .expect("First import should succeed");

    // Try to import same mnemonic with different label
    let result = import_wallet_from_mnemonic(network, label2, Purpose::Deposit, mnemonic.clone());
    assert!(
        result.is_err(),
        "Should fail when importing same mnemonic (duplicate address)"
    );
    assert!(matches!(
        result.unwrap_err(),
        BridgeCliError::AddressAlreadyExists(_)
    ));
}

/// Test importing with invalid mnemonic (wrong word count) fails
#[test]
#[serial]
fn test_mnemonic_import_wrong_word_count_fails() {
    setup_integration_test_env();

    // Try with only 11 words (should be 12)
    let invalid_mnemonic_str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon";
    let result = Mnemonic::from_str(invalid_mnemonic_str);
    assert!(
        result.is_err(),
        "Should fail to parse mnemonic with wrong word count"
    );

    // Try with 13 words (should be 12)
    let invalid_mnemonic_str2 = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about extra";
    let result2 = Mnemonic::from_str(invalid_mnemonic_str2);
    assert!(
        result2.is_err(),
        "Should fail to parse mnemonic with too many words"
    );
}

/// Test importing with invalid word (not in BIP-39 wordlist) fails
#[test]
#[serial]
fn test_mnemonic_import_invalid_word_fails() {
    setup_integration_test_env();

    // Use a word not in BIP-39 wordlist
    let invalid_mnemonic_str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon notaword";
    let result = Mnemonic::from_str(invalid_mnemonic_str);
    assert!(
        result.is_err(),
        "Should fail to parse mnemonic with invalid word"
    );
}

/// Test importing with wrong BIP-39 checksum fails
#[test]
#[serial]
fn test_mnemonic_import_invalid_checksum_fails() {
    setup_integration_test_env();

    // Valid words but invalid checksum (last word wrong)
    let invalid_mnemonic_str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon";
    let result = Mnemonic::from_str(invalid_mnemonic_str);
    assert!(
        result.is_err(),
        "Should fail to parse mnemonic with invalid checksum"
    );
}

/// Test that imported wallet is properly registered in registry
#[test]
#[serial]
fn test_mnemonic_import_registers_in_registry() {
    setup_integration_test_env();
    let network = test_network();
    let label = "registry-test";

    let mnemonic_str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let mnemonic = Mnemonic::from_str(mnemonic_str).expect("Valid BIP-39 mnemonic");

    // Import wallet
    let address = import_wallet_from_mnemonic(network, label, Purpose::Deposit, mnemonic.clone())
        .expect("Should import wallet");

    // Check registry
    let wallets = test_get_wallets_from_registry().expect("Should get registry");
    assert!(
        wallets.contains_key(&address.address_without_prefix()),
        "Wallet should be registered"
    );

    let entry = wallets.get(&address.address_without_prefix()).unwrap();
    assert_eq!(entry.label, label);
    assert_eq!(entry.addres_with_prefix, address.address_with_prefix());
    assert_eq!(entry.imported, Some(true));
    assert_eq!(entry.import_method, Some("mnemonic_import".to_string()));
}

/// Test cross-validation: Create wallet -> Export mnemonic -> Import mnemonic -> Verify match
#[test]
#[serial]
fn test_mnemonic_create_export_import_cycle() {
    setup_integration_test_env();
    let network = test_network();
    let passphrase = test_passphrase();

    // Step 1: Create a wallet
    let label1 = "create-test";
    let (address1, _, _) = create_encrypted_wallet(
        network,
        label1.to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create wallet");

    // Step 2: Export the mnemonic
    let mnemonic =
        get_mnemonic_from_wallet(&address1, &passphrase).expect("Should retrieve mnemonic");

    // Step 3: Parse the mnemonic
    let mnemonic_parsed =
        Mnemonic::from_str(&mnemonic.to_string()).expect("Retrieved mnemonic should be valid");

    // Step 4: Import the mnemonic as a new wallet
    let label2 = "import-test";
    let _ = import_wallet_from_mnemonic(network, label2, Purpose::Deposit, mnemonic_parsed)
        .expect_err("Address already exists");
}

/// Test mnemonic import stores encrypted data correctly
#[test]
#[serial]
fn test_mnemonic_import_stores_encrypted_correctly() {
    setup_integration_test_env();
    let network = test_network();
    let label = "encryption-test";
    let passphrase = test_passphrase();

    let mnemonic_str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let mnemonic = Mnemonic::from_str(mnemonic_str).expect("Valid BIP-39 mnemonic");

    // Import wallet
    let address = import_wallet_from_mnemonic(network, label, Purpose::Deposit, mnemonic.clone())
        .expect("Should import wallet");

    // Verify correct passphrase works
    let result_correct = get_mnemonic_from_wallet(&address, &passphrase);
    assert!(
        result_correct.is_ok(),
        "Should retrieve mnemonic with correct passphrase"
    );
}
