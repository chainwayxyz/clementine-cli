//! Async integration tests for Mempool API operations

use clementine_cli::__test_helpers::*;

use serial_test::serial;
use std::str::FromStr;
use url::Url;

use crate::common::mock_servers::mempool_api::*;
use crate::common::mock_servers::start_mock_server;

// Use test helpers to access API utils
use clementine_cli::__test_helpers::{
    test_broadcast_recovery_tx, test_get_block_height_for_tx, test_get_tx_details,
};

#[tokio::test]
#[serial]
async fn test_get_tx_details_success() {
    setup_integration_test_env();

    // Start mock server
    let mock_server = start_mock_server().await;

    let txid = "1111111111111111111111111111111111111111111111111111111111111111";
    let block_hash = "0000000000000000000000000000000000000000000000000000000000000001";
    let block_height = 100u64;

    // Mock transaction info
    mock_tx_info_success(&mock_server, txid, block_height, block_hash).await;

    // Create test transaction and block
    let test_tx = create_test_transaction();
    let test_block = create_test_block();

    // Mock transaction hex
    let tx_hex = bitcoin::consensus::serialize(&test_tx);
    let tx_hex_string = hex::encode(&tx_hex);
    mock_tx_hex_success(&mock_server, txid, &tx_hex_string).await;

    // Mock block raw data
    let block_bytes = bitcoin::consensus::serialize(&test_block);
    mock_block_raw_success(&mock_server, block_hash, block_bytes).await;

    // Configure to use mock mempool API
    let mut config = test_config();
    config.mempool_api_url =
        Some(Url::parse(&mock_server.uri()).expect("Should parse mock server URL"));
    config.bitcoin_config = None; // Disable RPC fallback

    let txid_parsed = bitcoin::Txid::from_str(txid).expect("Should parse txid");

    // Call get_tx_details
    let result = test_get_tx_details(&txid_parsed, &config).await;

    // Verify success
    assert!(
        result.is_ok(),
        "get_tx_details should succeed: {:?}",
        result.err()
    );
    let (tx, block, height) = result.unwrap();
    assert_eq!(tx.version, test_tx.version);
    assert_eq!(height, block_height as u32);
    assert_eq!(block.block_hash(), test_block.block_hash());
}

#[tokio::test]
#[serial]
async fn test_get_tx_details_not_found() {
    setup_integration_test_env();

    // Start mock server
    let mock_server = start_mock_server().await;

    let txid = "2222222222222222222222222222222222222222222222222222222222222222";

    // Mock transaction not found
    mock_tx_not_found(&mock_server, txid).await;

    // Configure to use mock mempool API
    let mut config = test_config();
    config.mempool_api_url =
        Some(Url::parse(&mock_server.uri()).expect("Should parse mock server URL"));
    config.bitcoin_config = None; // Disable RPC fallback

    let txid_parsed = bitcoin::Txid::from_str(txid).expect("Should parse txid");

    // Call get_tx_details
    let result = test_get_tx_details(&txid_parsed, &config).await;

    // Verify error
    assert!(
        result.is_err(),
        "get_tx_details should fail for non-existent transaction"
    );
}

#[tokio::test]
#[serial]
async fn test_get_block_height_for_tx_success() {
    setup_integration_test_env();

    // Start mock server
    let mock_server = start_mock_server().await;

    let txid = "3333333333333333333333333333333333333333333333333333333333333333";
    let block_hash = "0000000000000000000000000000000000000000000000000000000000000002";
    let block_height = 200u64;

    // Mock transaction info
    mock_tx_info_success(&mock_server, txid, block_height, block_hash).await;

    // Configure to use mock mempool API
    let mut config = test_config();
    config.mempool_api_url =
        Some(Url::parse(&mock_server.uri()).expect("Should parse mock server URL"));
    config.bitcoin_config = None; // Disable RPC fallback

    let txid_parsed = bitcoin::Txid::from_str(txid).expect("Should parse txid");

    // Call get_block_height_for_tx
    let result = test_get_block_height_for_tx(&txid_parsed, &config).await;

    // Verify success
    assert!(
        result.is_ok(),
        "get_block_height_for_tx should succeed: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap(), block_height);
}

#[tokio::test]
#[serial]
async fn test_broadcast_recovery_tx_success() {
    setup_integration_test_env();

    // Start mock server
    let mock_server = start_mock_server().await;

    let expected_txid = "4444444444444444444444444444444444444444444444444444444444444444";

    // Mock successful broadcast
    mock_broadcast_tx_success(&mock_server, expected_txid).await;

    // Configure to use mock mempool API
    let mut config = test_config();
    config.mempool_api_url =
        Some(Url::parse(&mock_server.uri()).expect("Should parse mock server URL"));
    config.bitcoin_config = None; // Disable RPC fallback

    // Create a test transaction
    let test_tx = create_test_transaction();
    let raw_tx = hex::encode(bitcoin::consensus::serialize(&test_tx));

    // Call broadcast_recovery_tx
    let result = test_broadcast_recovery_tx(&config, raw_tx).await;

    // Verify success
    assert!(
        result.is_ok(),
        "broadcast_recovery_tx should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
#[serial]
async fn test_broadcast_recovery_tx_invalid_tx() {
    setup_integration_test_env();

    // Start mock server
    let mock_server = start_mock_server().await;

    // Mock broadcast error
    mock_broadcast_tx_error(&mock_server, "Transaction decode failed").await;

    // Configure to use mock mempool API
    let mut config = test_config();
    config.mempool_api_url =
        Some(Url::parse(&mock_server.uri()).expect("Should parse mock server URL"));
    config.bitcoin_config = None; // Disable RPC fallback

    let invalid_tx = "invalid_hex_string";

    // Call broadcast_recovery_tx
    let result = test_broadcast_recovery_tx(&config, invalid_tx.to_string()).await;

    // Verify error
    assert!(
        result.is_err(),
        "broadcast_recovery_tx should fail for invalid transaction"
    );
}

#[tokio::test]
#[serial]
async fn test_mempool_api_unavailable_no_fallback() {
    setup_integration_test_env();

    // Configure with invalid mempool URL and no RPC fallback
    let mut config = test_config();
    config.mempool_api_url = Some(Url::parse("http://localhost:1").expect("Should parse URL"));
    config.bitcoin_config = None; // No fallback

    let txid =
        bitcoin::Txid::from_str("5555555555555555555555555555555555555555555555555555555555555555")
            .expect("Should parse txid");

    // Call get_tx_details - should fail with no fallback
    let result = test_get_tx_details(&txid, &config).await;

    // Verify error (connection refused or timeout)
    assert!(
        result.is_err(),
        "get_tx_details should fail when mempool API is unavailable and no RPC fallback"
    );
}
