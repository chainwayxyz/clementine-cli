//! Integration tests for complete deposit workflow scenarios

use clementine_cli::__test_helpers::*;

use serial_test::serial;
use url::Url;

use clementine_cli::__test_helpers::{test_backend_deposit_status, test_create_deposit_account};
use clementine_cli::create_encrypted_wallet;
use clementine_cli::wallet::Purpose;

use crate::common::mock_servers::bridge_backend::*;
use crate::common::mock_servers::start_mock_server;

#[tokio::test]
#[serial]
async fn test_deposit_workflow_new_to_completed() {
    setup_integration_test_env();
    let network = test_network();

    // Step 1: Create deposit wallet
    let (deposit_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "deposit_workflow_wallet".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create deposit wallet");

    // Start mock server
    let mock_server = start_mock_server().await;

    // Step 2: Check initial status - should be empty
    mock_deposit_status_empty(&mock_server).await;

    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    let result = test_backend_deposit_status(&deposit_address.address, &config).await;
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap().len(),
        0,
        "Should have no deposits initially"
    );

    // Step 3: Simulate deposit appearing with "new" status
    mock_deposit_status_success(&mock_server, "new").await;

    let result = test_backend_deposit_status(&deposit_address.address, &config).await;
    assert!(result.is_ok());
    let statuses = result.unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].status, "new");

    // Step 4: Simulate deposit progressing to "minted" (completed)
    mock_deposit_status_success(&mock_server, "minted").await;

    let result = test_backend_deposit_status(&deposit_address.address, &config).await;
    assert!(result.is_ok());
    let statuses = result.unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].status, "minted");
}

#[tokio::test]
#[serial]
async fn test_multiple_deposits_for_same_address() {
    setup_integration_test_env();
    let network = test_network();

    // Create deposit wallet
    let (deposit_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "multi_deposit_wallet".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create deposit wallet");

    // Start mock server
    let mock_server = start_mock_server().await;

    // Mock multiple deposits
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};

    Mock::given(method("GET"))
        .and(path("/deposits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": 1,
                "status": "new",
                "txid": "1111111111111111111111111111111111111111111111111111111111111111",
                "evm_addr": "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb1",
                "move_tx_raw": "0x",
                "move_txid": "2222222222222222222222222222222222222222222222222222222222222222",
                "created_at": "2023-01-01T00:00:00Z",
                "mint_txid": ""
            },
            {
                "id": 2,
                "status": "minted",
                "txid": "3333333333333333333333333333333333333333333333333333333333333333",
                "evm_addr": "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb1",
                "move_tx_raw": "0x",
                "move_txid": "4444444444444444444444444444444444444444444444444444444444444444",
                "created_at": "2023-01-02T00:00:00Z",
                "mint_txid": "5555555555555555555555555555555555555555555555555555555555555555"
            }
        ])))
        .mount(&mock_server)
        .await;

    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    let result = test_backend_deposit_status(&deposit_address.address, &config).await;
    assert!(result.is_ok());
    let statuses = result.unwrap();
    assert_eq!(statuses.len(), 2, "Should have 2 deposits");
    assert_eq!(statuses[0].status, "new");
    assert_eq!(statuses[1].status, "minted");
}

#[tokio::test]
#[serial]
async fn test_deposit_status_transition_sequence() {
    setup_integration_test_env();
    let network = test_network();

    // Create deposit wallet
    let (deposit_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "status_transition_wallet".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create deposit wallet");

    let mock_server = start_mock_server().await;
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    // Sequence of status transitions: new -> flushing_initiated -> sent -> minted
    let status_sequence = vec!["new", "flushing_initiated", "sent", "minted"];

    for status in status_sequence {
        mock_deposit_status_success(&mock_server, status).await;

        let result = test_backend_deposit_status(&deposit_address.address, &config).await;
        assert!(result.is_ok(), "Should fetch status for: {}", status);
        let statuses = result.unwrap();
        assert_eq!(statuses[0].status, status);
    }
}

#[tokio::test]
#[serial]
async fn test_deposit_backend_error_recovery() {
    setup_integration_test_env();
    let network = test_network();

    // Create deposit wallet
    let (deposit_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "error_recovery_wallet".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create deposit wallet");

    let mock_server = start_mock_server().await;
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    // First attempt: backend error
    mock_deposit_status_error(&mock_server).await;

    let result = test_backend_deposit_status(&deposit_address.address, &config).await;
    assert!(result.is_err(), "Should fail with backend error");

    // Second attempt: backend recovered
    mock_deposit_status_success(&mock_server, "new").await;

    let result = test_backend_deposit_status(&deposit_address.address, &config).await;
    assert!(result.is_ok(), "Should succeed after backend recovery");
}

#[tokio::test]
#[serial]
async fn test_create_deposit_account_idempotency() {
    setup_integration_test_env();
    let network = bitcoin::Network::Testnet;

    // Create recovery wallet
    let (recovery_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "idempotent_deposit".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create recovery wallet");

    let mock_server = start_mock_server().await;
    let citrea_address = test_citrea_address();
    let mut config = test_config();
    config.network = network;
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    // Calculate the deposit address
    use clementine_cli::__test_helpers::test_calculate_deposit_address;
    let (deposit_addr, _) =
        test_calculate_deposit_address(&citrea_address, &recovery_address.address, &config)
            .expect("Should calculate deposit address");

    // Mock same response for both calls
    mock_create_deposit_account_success(
        &mock_server,
        &deposit_addr.to_string(),
        &citrea_address.to_string(),
    )
    .await;

    // Call create_deposit_account twice
    let result1 =
        test_create_deposit_account(&citrea_address, &recovery_address.address, &config).await;
    assert!(result1.is_ok());

    let result2 =
        test_create_deposit_account(&citrea_address, &recovery_address.address, &config).await;
    assert!(result2.is_ok());

    // Both should return the same address
    assert_eq!(result1.unwrap().to_string(), result2.unwrap().to_string());
}
