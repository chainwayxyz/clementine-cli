//! Async integration tests for deposit operations

use clementine_cli::__test_helpers::*;

use clementine_cli::wallet::Purpose;
use clementine_cli::{BridgeCliConfig, create_encrypted_wallet, get_deposit_address};
use serial_test::serial;
use url::Url;

use crate::common::mock_servers::bridge_backend::*;
use crate::common::mock_servers::start_mock_server;

#[tokio::test]
#[serial]
async fn test_get_deposit_address_success() {
    setup_integration_test_env();
    let network = bitcoin::Network::Testnet4; // Use testnet for backend testing

    // Create recovery wallet (Deposit purpose) on testnet
    let (recovery_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "recovery_wallet_async".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create recovery wallet");

    // Start mock server
    let mock_server = start_mock_server().await;

    // Get the actual calculated address to use in mock
    let citrea_address = test_citrea_address();
    let mut config = BridgeCliConfig::defaults_for(network);
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    // Calculate what the deposit address should be
    use clementine_cli::__test_helpers::test_calculate_deposit_address;
    let (expected_deposit_addr, _) =
        test_calculate_deposit_address(&citrea_address, &recovery_address.address, &config)
            .expect("Should calculate deposit address");

    // Mock backend returning the same address we calculated
    mock_create_deposit_account_success(
        &mock_server,
        &expected_deposit_addr.to_string(),
        &citrea_address.to_string(),
    )
    .await;

    // Call get_deposit_address
    let result = get_deposit_address(&citrea_address, &recovery_address, &config).await;

    // Verify success
    assert!(
        result.is_ok(),
        "get_deposit_address should succeed: {:?}",
        result.err()
    );
    let deposit_addr = result.unwrap();
    assert_eq!(deposit_addr.to_string(), expected_deposit_addr.to_string());
}

#[tokio::test]
#[serial]
async fn test_get_deposit_address_regtest_skips_backend() {
    setup_integration_test_env();
    let network = test_network(); // Regtest

    // Create recovery wallet
    let (recovery_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "recovery_wallet_regtest".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create recovery wallet");

    // Use regtest config (no need to mock backend)
    let config = test_config();
    let citrea_address = test_citrea_address();

    // Call get_deposit_address - should not call backend for regtest
    let result = get_deposit_address(&citrea_address, &recovery_address, &config).await;

    // Verify success (calculated address returned without backend check)
    assert!(
        result.is_ok(),
        "get_deposit_address should succeed for regtest"
    );
}

#[tokio::test]
#[serial]
async fn test_get_deposit_address_backend_error() {
    setup_integration_test_env();
    let network = test_network();

    // Create recovery wallet
    let (recovery_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "recovery_wallet_error".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create recovery wallet");

    // Start mock server
    let mock_server = start_mock_server().await;

    // Mock backend error
    mock_create_deposit_account_error(&mock_server).await;

    // Configure to use mock server
    let mut config = test_config();
    config.network = bitcoin::Network::Testnet;
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    let citrea_address = test_citrea_address();

    // Call get_deposit_address
    let result = get_deposit_address(&citrea_address, &recovery_address, &config).await;

    // Verify error
    assert!(
        result.is_err(),
        "get_deposit_address should fail with backend error"
    );
}

#[tokio::test]
#[serial]
async fn test_get_deposit_address_invalid_purpose() {
    setup_integration_test_env();
    let network = test_network();

    // Create wallet with WRONG purpose (Withdrawal instead of Deposit)
    let (wrong_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "wrong_purpose_wallet".to_string(),
        Purpose::Withdrawal,
        test_passphrase(),
    )
    .expect("Should create wallet");

    let config = test_config();
    let citrea_address = test_citrea_address();

    // Call get_deposit_address with wrong purpose wallet
    let result = get_deposit_address(&citrea_address, &wrong_address, &config).await;

    // Verify error due to purpose mismatch
    assert!(
        result.is_err(),
        "get_deposit_address should fail with wrong purpose"
    );
}

#[tokio::test]
#[serial]
async fn test_get_deposit_address_mismatch_error() {
    setup_integration_test_env();
    let network = bitcoin::Network::Testnet4;

    // Create recovery wallet
    let (recovery_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "recovery_wallet_mismatch".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create recovery wallet");

    // Start mock server
    let mock_server = start_mock_server().await;

    let citrea_address = test_citrea_address();
    let mut config = BridgeCliConfig::defaults_for(network);
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    // Calculate correct address
    use clementine_cli::__test_helpers::test_calculate_deposit_address;
    let (correct_addr, _) =
        test_calculate_deposit_address(&citrea_address, &recovery_address.address, &config)
            .expect("Should calculate deposit address");

    // Mock backend returning DIFFERENT address (just append an 'x' to make it different)
    let wrong_deposit_addr = format!("{}x", correct_addr.to_string());
    mock_create_deposit_account_success(
        &mock_server,
        &wrong_deposit_addr,
        &citrea_address.to_string(),
    )
    .await;

    // Call get_deposit_address
    let result = get_deposit_address(&citrea_address, &recovery_address, &config).await;

    // Verify error due to address mismatch - will fail at parse stage since we appended 'x'
    assert!(
        result.is_err(),
        "get_deposit_address should fail when backend returns different address"
    );
}
