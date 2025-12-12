//! Integration tests for complete withdrawal workflow scenarios

use clementine_cli::__test_helpers::*;

use bitcoin::OutPoint;
use serial_test::serial;
use std::str::FromStr;
use url::Url;

use clementine_cli::__test_helpers::{
    test_backend_withdrawal_status, test_send_withdrawal_signature_to_operators,
};

use crate::common::mock_servers::bridge_backend::*;
use crate::common::mock_servers::start_mock_server;

#[tokio::test]
#[serial]
async fn test_withdrawal_workflow_new_to_completed() {
    setup_integration_test_env();

    let mock_server = start_mock_server().await;
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    let outpoint =
        OutPoint::from_str("1111111111111111111111111111111111111111111111111111111111111111:0")
            .expect("Should parse outpoint");

    // Step 1: Check withdrawal status - new
    mock_withdrawal_status_success(&mock_server, "new").await;

    let result = test_backend_withdrawal_status(outpoint, &config).await;
    assert!(result.is_ok());
    let statuses = result.unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].status, "new");

    // Step 2: Submit signature
    mock_send_withdrawal_signature_success(&mock_server).await;

    let signature = "3044022001234567890123456789012345678901234567890123456789012345678901230220abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";
    let result = test_send_withdrawal_signature_to_operators(
        "bcrt1ptest",
        "bcrt1pdest",
        outpoint,
        signature,
        &config,
        100_000,
    )
    .await;
    assert!(result.is_ok(), "Signature submission should succeed");

    // Step 3: Check status after signature - should progress
    mock_withdrawal_status_success(&mock_server, "sending-to-optimistic-payout").await;

    let result = test_backend_withdrawal_status(outpoint, &config).await;
    assert!(result.is_ok());
    let statuses = result.unwrap();
    assert_eq!(statuses[0].status, "sending-to-optimistic-payout");

    // Step 4: Final status - completed
    mock_withdrawal_status_success(&mock_server, "completed").await;

    let result = test_backend_withdrawal_status(outpoint, &config).await;
    assert!(result.is_ok());
    let statuses = result.unwrap();
    assert_eq!(statuses[0].status, "completed");
}

#[tokio::test]
#[serial]
async fn test_withdrawal_signature_already_submitted() {
    setup_integration_test_env();

    let mock_server = start_mock_server().await;
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    let outpoint =
        OutPoint::from_str("2222222222222222222222222222222222222222222222222222222222222222:0")
            .expect("Should parse outpoint");

    // First submission: success
    mock_send_withdrawal_signature_success(&mock_server).await;

    let signature = "3044022001234567890123456789012345678901234567890123456789012345678901230220abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";
    let result = test_send_withdrawal_signature_to_operators(
        "bcrt1ptest",
        "bcrt1pdest",
        outpoint,
        signature,
        &config,
        100_000,
    )
    .await;
    assert!(result.is_ok());

    // Second submission: already exists
    mock_send_withdrawal_signature_already_exists(&mock_server).await;

    let result = test_send_withdrawal_signature_to_operators(
        "bcrt1ptest",
        "bcrt1pdest",
        outpoint,
        signature,
        &config,
        100_000,
    )
    .await;
    assert!(result.is_err(), "Should fail when signature already exists");
    assert!(format!("{:?}", result.unwrap_err()).contains("already"));
}

#[tokio::test]
#[serial]
async fn test_withdrawal_signature_before_confirmation() {
    setup_integration_test_env();

    let mock_server = start_mock_server().await;
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    let outpoint =
        OutPoint::from_str("3333333333333333333333333333333333333333333333333333333333333333:0")
            .expect("Should parse outpoint");

    // Try to submit signature for unconfirmed withdrawal
    mock_send_withdrawal_signature_not_found(&mock_server).await;

    let signature = "3044022001234567890123456789012345678901234567890123456789012345678901230220abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";
    let result = test_send_withdrawal_signature_to_operators(
        "bcrt1ptest",
        "bcrt1pdest",
        outpoint,
        signature,
        &config,
        100_000,
    )
    .await;

    assert!(result.is_err(), "Should fail for unconfirmed withdrawal");
    assert!(format!("{:?}", result.unwrap_err()).contains("not found"));
}

#[tokio::test]
#[serial]
async fn test_withdrawal_optimistic_payout_failed() {
    setup_integration_test_env();

    let mock_server = start_mock_server().await;
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    let outpoint =
        OutPoint::from_str("4444444444444444444444444444444444444444444444444444444444444444:0")
            .expect("Should parse outpoint");

    // Check withdrawal with optimistic-payout-failed status
    mock_withdrawal_status_success(&mock_server, "optimistic-payout-failed").await;

    let result = test_backend_withdrawal_status(outpoint, &config).await;
    assert!(result.is_ok());
    let statuses = result.unwrap();
    assert_eq!(statuses[0].status, "optimistic-payout-failed");
}

#[tokio::test]
#[serial]
async fn test_multiple_withdrawals_for_user() {
    setup_integration_test_env();

    let mock_server = start_mock_server().await;

    // Mock multiple withdrawals
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};

    Mock::given(method("GET"))
        .and(path("/withdrawals"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "idx": 1,
                "status": "new",
                "btc_payment_txid": "1111111111111111111111111111111111111111111111111111111111111111",
                "from_safe_withdraw": true,
                "optimistic_payout_started_at": null,
                "optimistic_payout_deadline_at": null,
                "created_at": "2023-01-01T00:00:00Z",
                "optimistic_payout_payment": null
            },
            {
                "idx": 2,
                "status": "completed",
                "btc_payment_txid": "2222222222222222222222222222222222222222222222222222222222222222",
                "from_safe_withdraw": false,
                "optimistic_payout_started_at": "2023-01-01T01:00:00Z",
                "optimistic_payout_deadline_at": "2023-01-02T01:00:00Z",
                "created_at": "2023-01-01T00:00:00Z",
                "optimistic_payout_payment": {
                    "tx_raw": "0x",
                    "txid": "3333333333333333333333333333333333333333333333333333333333333333"
                }
            }
        ])))
        .mount(&mock_server)
        .await;

    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    let outpoint =
        OutPoint::from_str("5555555555555555555555555555555555555555555555555555555555555555:0")
            .expect("Should parse outpoint");

    let result = test_backend_withdrawal_status(outpoint, &config).await;
    assert!(result.is_ok());
    let statuses = result.unwrap();
    assert_eq!(statuses.len(), 2, "Should have 2 withdrawals");
    assert_eq!(statuses[0].status, "new");
    assert_eq!(statuses[1].status, "completed");
}

#[tokio::test]
#[serial]
async fn test_withdrawal_status_transitions() {
    setup_integration_test_env();

    let mock_server = start_mock_server().await;
    let mut config = test_config();
    config.citrea_backend_endpoint =
        Url::parse(&mock_server.uri()).expect("Should parse mock server URL");

    let outpoint =
        OutPoint::from_str("6666666666666666666666666666666666666666666666666666666666666666:0")
            .expect("Should parse outpoint");

    // Test full status transition sequence
    let status_sequence = vec![
        "new",
        "sending-to-optimistic-payout",
        "sent-to-optimistic-payout",
        "optimistic-payout-failed", // Failed first attempt
        "sending-to-operator-withdraw",
        "sent-to-operator-withdraw",
        "completed",
    ];

    for status in status_sequence {
        mock_withdrawal_status_success(&mock_server, status).await;

        let result = test_backend_withdrawal_status(outpoint, &config).await;
        assert!(result.is_ok(), "Should fetch status for: {}", status);
        let statuses = result.unwrap();
        assert_eq!(statuses[0].status, status);
    }
}
