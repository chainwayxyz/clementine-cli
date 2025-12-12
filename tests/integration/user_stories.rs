//! Real-life user stories and edge case scenarios

use clementine_cli::__test_helpers::*;
//! 

use clementine_cli::__test_helpers::*;
//! This module tests complex, multi-step user workflows that mimic real-world usage,

use clementine_cli::__test_helpers::*;
//! including wallet restoration, file corruption, and registry desynchronization.

use clementine_cli::__test_helpers::*;

use serial_test::serial;
use std::fs;
use url::Url;

use clementine_cli::__test_helpers::test_backend_deposit_status;
use clementine_cli::wallet::Purpose;
use clementine_cli::{
    create_encrypted_wallet, get_clementine_home_dir_with_existence_check,
    import_wallet_from_mnemonic,
};

use crate::common::mock_servers::bridge_backend::*;
use crate::common::mock_servers::start_mock_server;

#[tokio::test]
#[serial]
async fn test_user_story_wallet_restoration() {
    // Scenario: User loses their wallet file but has the mnemonic. 
    // They install the CLI on a new machine (fresh temp dir), import, and check status.
    
    setup_integration_test_env();
    let network = test_network();
    let passphrase = test_passphrase();

    // 1. "Old Machine": Create a wallet and get the mnemonic
    let (original_address, mnemonic, _) = create_encrypted_wallet(
        network,
        "original_wallet".to_string(),
        Purpose::Deposit,
        passphrase.clone(),
    )
    .expect("Should create original wallet");

    // 2. Simulate "New Machine" or Data Loss
    // We can't easily switch the thread-local temp dir, but we can delete the wallet file
    // and remove it from the registry to simulate a fresh start.
    // However, the registry is also in the temp dir. 
    // Let's just delete the specific wallet file to simulate "file loss" 
    // and then import it as a "restored" wallet.
    
    let home_dir = get_clementine_home_dir_with_existence_check().unwrap();
    let wallet_path = home_dir
        .join("keys")
        .join(format!("wallet_{}.json", original_address.address_without_prefix()));
    
    // verify it exists first
    assert!(wallet_path.exists());
    
    // Simulate accidental deletion
    fs::remove_file(&wallet_path).expect("Should delete wallet file");
    
    // 3. User imports the wallet from mnemonic
    let (restored_address, _, _) = import_wallet_from_mnemonic(
        &mnemonic,
        passphrase,
        "restored_wallet".to_string(),
        network,
        Purpose::Deposit,
    )
    .expect("Should restore wallet from mnemonic");

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
    
    // 1. Create a wallet
    let (address, _, _) = create_encrypted_wallet(
        network,
        "desync_wallet".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create wallet");

    // 2. Sabotage: Delete the file manually
    let home_dir = get_clementine_home_dir_with_existence_check().unwrap();
    let wallet_path = home_dir
        .join("keys")
        .join(format!("wallet_{}.json", address.address_without_prefix()));
    
    fs::remove_file(&wallet_path).expect("Should delete wallet file");
    assert!(!wallet_path.exists());

    // 3. Attempt an operation that requires the key (e.g., getting private key or signing)
    // We'll use a lower-level helper that attempts to load the key.
    use clementine_cli::__test_helpers::test_get_private_key_from_wallet;
    
    let result = test_get_private_key_from_wallet(
        &address, 
        &test_passphrase()
    );

    // 4. Expect a specific error (WalletFileNotFound)
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(format!("{:?}", err).contains("WalletFileNotFound"));
}
