//! Async integration tests for backend status operations

use clementine_cli::__test_helpers::*;

use bitcoin::OutPoint;
use clementine_cli::create_encrypted_wallet;
use clementine_cli::wallet::Purpose;
use serial_test::serial;
use std::str::FromStr;
use url::Url;

use crate::common::mock_servers::bridge_backend::*;
use crate::common::mock_servers::start_mock_server;

// Use test helpers to access internal functions
use clementine_cli::__test_helpers::{
    test_backend_deposit_status, test_backend_withdrawal_status,
    test_send_withdrawal_signature_to_operators,
};

#[tokio::test]
#[serial]
async fn test_deposit_status_success() {
    setup_integration_test_env();
    let network = test_network();

    // Create deposit wallet
    let (deposit_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "deposit_status_wallet".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create deposit wallet");

    // Start mock server
    let mock_server = start_mock_server().await;

    // Mock successful deposit status
    mock_deposit_status_success(&mock_server, "new").await;

    // Configure to use mock server
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    // Call backend_deposit_status
    let result = test_backend_deposit_status(&deposit_address.address, &config).await;

    // Verify success
    assert!(
        result.is_ok(),
        "backend_deposit_status should succeed: {:?}",
        result.err()
    );
    let statuses = result.unwrap();
    assert_eq!(statuses.len(), 1, "Should return one deposit status");
    assert_eq!(statuses[0].status, "new", "Status should be 'new'");
}

#[tokio::test]
#[serial]
async fn test_deposit_status_empty() {
    setup_integration_test_env();
    let network = test_network();

    // Create deposit wallet
    let (deposit_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "deposit_status_empty_wallet".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create deposit wallet");

    // Start mock server
    let mock_server = start_mock_server().await;

    // Mock empty deposit status
    mock_deposit_status_empty(&mock_server).await;

    // Configure to use mock server
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    // Call backend_deposit_status
    let result = test_backend_deposit_status(&deposit_address.address, &config).await;

    // Verify success with empty list
    assert!(result.is_ok(), "backend_deposit_status should succeed");
    let statuses = result.unwrap();
    assert_eq!(statuses.len(), 0, "Should return empty list");
}

#[tokio::test]
#[serial]
async fn test_deposit_status_backend_error() {
    setup_integration_test_env();
    let network = test_network();

    // Create deposit wallet
    let (deposit_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "deposit_status_error_wallet".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create deposit wallet");

    // Start mock server
    let mock_server = start_mock_server().await;

    // Mock backend error
    mock_deposit_status_error(&mock_server).await;

    // Configure to use mock server
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    // Call backend_deposit_status
    let result = test_backend_deposit_status(&deposit_address.address, &config).await;

    // Verify error
    assert!(
        result.is_err(),
        "backend_deposit_status should fail with backend error"
    );
}

#[tokio::test]
#[serial]
async fn test_withdrawal_status_success() {
    setup_integration_test_env();

    // Start mock server
    let mock_server = start_mock_server().await;

    // Mock successful withdrawal status
    mock_withdrawal_status_success(&mock_server, "new").await;

    // Configure to use mock server
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    // Create test outpoint
    let outpoint =
        OutPoint::from_str("1111111111111111111111111111111111111111111111111111111111111111:0")
            .expect("Should parse outpoint");

    // Call backend_withdrawal_status
    let result = test_backend_withdrawal_status(outpoint, &config).await;

    // Verify success
    assert!(
        result.is_ok(),
        "backend_withdrawal_status should succeed: {:?}",
        result.err()
    );
    let statuses = result.unwrap();
    assert_eq!(statuses.len(), 1, "Should return one withdrawal status");
    assert_eq!(statuses[0].status, "new", "Status should be 'new'");
}

#[tokio::test]
#[serial]
async fn test_send_withdrawal_signature_success() {
    setup_integration_test_env();

    // Start mock server
    let mock_server = start_mock_server().await;

    // Mock successful signature submission
    mock_send_withdrawal_signature_success(&mock_server).await;

    // Configure to use mock server
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    // Create test data
    let outpoint =
        OutPoint::from_str("1111111111111111111111111111111111111111111111111111111111111111:0")
            .expect("Should parse outpoint");
    let signer_addr = "bcrt1ptest";
    let dest_addr = "bcrt1pdest";
    let signature = "3044022001234567890123456789012345678901234567890123456789012345678901230220abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";
    let amount = 100_000;

    // Call send_withdrawal_signature_to_operators
    let result = test_send_withdrawal_signature_to_operators(
        signer_addr,
        dest_addr,
        outpoint,
        signature,
        &config,
        amount,
    )
    .await;

    // Verify success
    assert!(
        result.is_ok(),
        "send_withdrawal_signature_to_operators should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
#[serial]
async fn test_send_withdrawal_signature_not_found() {
    setup_integration_test_env();

    // Start mock server
    let mock_server = start_mock_server().await;

    // Mock withdrawal not found
    mock_send_withdrawal_signature_not_found(&mock_server).await;

    // Configure to use mock server
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    // Create test data
    let outpoint =
        OutPoint::from_str("1111111111111111111111111111111111111111111111111111111111111111:0")
            .expect("Should parse outpoint");
    let signer_addr = "bcrt1ptest";
    let dest_addr = "bcrt1pdest";
    let signature = "3044022001234567890123456789012345678901234567890123456789012345678901230220abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";
    let amount = 100_000;

    // Call send_withdrawal_signature_to_operators
    let result = test_send_withdrawal_signature_to_operators(
        signer_addr,
        dest_addr,
        outpoint,
        signature,
        &config,
        amount,
    )
    .await;

    // Verify error
    assert!(
        result.is_err(),
        "send_withdrawal_signature_to_operators should fail"
    );
    let error = result.unwrap_err();
    assert!(
        format!("{:?}", error).contains("not found"),
        "Error should mention not found"
    );
}

#[tokio::test]
#[serial]
async fn test_send_withdrawal_signature_already_exists() {
    setup_integration_test_env();

    // Start mock server
    let mock_server = start_mock_server().await;

    // Mock signature already exists
    mock_send_withdrawal_signature_already_exists(&mock_server).await;

    // Configure to use mock server
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    // Create test data
    let outpoint =
        OutPoint::from_str("1111111111111111111111111111111111111111111111111111111111111111:0")
            .expect("Should parse outpoint");
    let signer_addr = "bcrt1ptest";
    let dest_addr = "bcrt1pdest";
    let signature = "3044022001234567890123456789012345678901234567890123456789012345678901230220abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";
    let amount = 100_000;

    // Call send_withdrawal_signature_to_operators
    let result = test_send_withdrawal_signature_to_operators(
        signer_addr,
        dest_addr,
        outpoint,
        signature,
        &config,
        amount,
    )
    .await;

    // Verify error
    assert!(
        result.is_err(),
        "send_withdrawal_signature_to_operators should fail"
    );
    let error = result.unwrap_err();
    assert!(
        format!("{:?}", error).contains("already"),
        "Error should mention already exists"
    );
}
