//! Real-life user stories and edge case scenarios
//!
//! This module tests complex, multi-step user workflows that mimic real-world usage,
//! including wallet restoration, file corruption, and registry desynchronization.

use clementine_cli::__test_helpers::*;

use serial_test::serial;
use std::fs;
use url::Url;

use clementine_cli::__test_helpers::test_backend_deposit_status;
use clementine_cli::wallet::Purpose;
use clementine_cli::{create_encrypted_wallet, import_wallet_from_private_key};

use crate::common::mock_servers::bridge_backend::*;
use crate::common::mock_servers::start_mock_server;

#[tokio::test]
#[serial]
async fn test_user_story_wallet_restoration() {
    // Scenario: User loses their wallet file but has the private key.
    // They install the CLI on a new machine (fresh temp dir), import, and check status.

    setup_integration_test_env();
    let network = test_network();

    // 1. "Old Machine": Create a wallet and get the private key
    let (original_address, _mnemonic, wallet_path) = create_encrypted_wallet(
        network,
        "original_wallet".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create original wallet");

    // Extract private key before "losing" the wallet
    use clementine_cli::__test_helpers::test_get_private_key_from_wallet;
    let private_key = test_get_private_key_from_wallet(&original_address, &test_passphrase())
        .expect("Should get private key");

    // 2. Simulate "New Machine" or Data Loss
    // verify it exists first
    assert!(wallet_path.exists());

    // Simulate moving to a new machine by clearing the temp directory
    // This removes both the wallet file and the registry
    setup_integration_test_env();

    // 3. User imports the wallet from private key
    let restored_address = import_wallet_from_private_key(
        network,
        "restored_wallet",
        Purpose::Deposit,
        secrecy::SecretBox::new(Box::new(hex::encode(
            private_key.as_ref_inner().secret_bytes(),
        ))),
        test_passphrase(),
    )
    .expect("Should restore wallet from private key");

    assert_eq!(
        original_address.address, restored_address.address,
        "Restored address should match original"
    );

    // 4. User checks backend status for the restored wallet
    let mock_server = start_mock_server().await;
    mock_deposit_status_success(&mock_server, "minted").await;

    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    let result = test_backend_deposit_status(&restored_address.address, &config).await;

    assert!(result.is_ok());
    let statuses = result.unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].status, "minted");
}

#[tokio::test]
#[serial]
async fn test_user_story_registry_desync_file_missing() {
    // Scenario: The internal registry thinks a wallet exists, but the file is gone.
    // This happens if a user manually deletes files in ~/.clementine/keys/

    setup_integration_test_env();
    let network = test_network();

    // 1. Create a wallet with a unique label to avoid conflicts
    let unique_label = format!(
        "desync_wallet_{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    let (address, _, path) =
        create_encrypted_wallet(network, unique_label, Purpose::Deposit, test_passphrase())
            .expect("Should create wallet");

    // 2. Sabotage: Delete the file manually
    fs::remove_file(&path).expect("Should delete wallet file");
    assert!(!path.exists());

    // 3. Attempt an operation that requires the key (e.g., getting private key or signing)
    // We'll use a lower-level helper that attempts to load the key.
    use clementine_cli::__test_helpers::test_get_private_key_from_wallet;

    let result = test_get_private_key_from_wallet(&address, &test_passphrase());

    // 4. Expect a specific error (WalletNotFound - the wallet file is missing)
    assert!(result.is_err());
    let err = result.err().unwrap();
    let err_string = format!("{:?}", err);
    assert!(
        err_string.contains("WalletNotFound"),
        "Expected WalletNotFound error, got: {}",
        err_string
    );
}
