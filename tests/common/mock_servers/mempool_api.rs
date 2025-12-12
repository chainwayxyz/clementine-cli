//! Mock helpers for Mempool API

use bitcoin::{Block, Transaction};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mock successful transaction info request
/// GET /tx/{txid}
pub async fn mock_tx_info_success(
    mock_server: &MockServer,
    txid: &str,
    block_height: u64,
    block_hash: &str,
) {
    let path_str = format!("/tx/{}", txid);
    Mock::given(method("GET"))
        .and(path(path_str))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "txid": txid,
            "status": {
                "confirmed": true,
                "block_height": block_height,
                "block_hash": block_hash,
                "block_time": 1234567890
            },
            "vout": [
                {
                    "value": 100000,
                    "scriptpubkey_address": "bcrt1qtest"
                }
            ]
        })))
        .mount(mock_server)
        .await;
}

/// Mock transaction not found (404)
pub async fn mock_tx_not_found(mock_server: &MockServer, txid: &str) {
    let path_str = format!("/tx/{}", txid);
    Mock::given(method("GET"))
        .and(path(path_str))
        .respond_with(ResponseTemplate::new(404).set_body_string("Transaction not found"))
        .mount(mock_server)
        .await;
}

/// Mock transaction hex request
/// GET /tx/{txid}/hex
pub async fn mock_tx_hex_success(mock_server: &MockServer, txid: &str, tx_hex: &str) {
    let path_str = format!("/tx/{}/hex", txid);
    Mock::given(method("GET"))
        .and(path(path_str))
        .respond_with(ResponseTemplate::new(200).set_body_string(tx_hex))
        .mount(mock_server)
        .await;
}

/// Mock block raw data request
/// GET /block/{blockhash}/raw
pub async fn mock_block_raw_success(
    mock_server: &MockServer,
    block_hash: &str,
    block_bytes: Vec<u8>,
) {
    let path_str = format!("/block/{}/raw", block_hash);
    Mock::given(method("GET"))
        .and(path(path_str))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(block_bytes))
        .mount(mock_server)
        .await;
}

/// Mock successful transaction broadcast
/// POST /tx
pub async fn mock_broadcast_tx_success(mock_server: &MockServer, expected_txid: &str) {
    Mock::given(method("POST"))
        .and(path("/tx"))
        .respond_with(ResponseTemplate::new(200).set_body_string(expected_txid))
        .mount(mock_server)
        .await;
}

/// Mock transaction broadcast failure
pub async fn mock_broadcast_tx_error(mock_server: &MockServer, error_msg: &str) {
    Mock::given(method("POST"))
        .and(path("/tx"))
        .respond_with(ResponseTemplate::new(400).set_body_string(error_msg))
        .mount(mock_server)
        .await;
}

/// Create a minimal valid Bitcoin transaction for testing
pub fn create_test_transaction() -> Transaction {
    use bitcoin::{OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness};
    use std::str::FromStr;

    Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: bitcoin::blockdata::locktime::absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: Txid::from_str(
                    "1111111111111111111111111111111111111111111111111111111111111111",
                )
                .unwrap(),
                vout: 0,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: bitcoin::Amount::from_sat(100_000),
            script_pubkey: ScriptBuf::new(),
        }],
    }
}

/// Create a minimal valid Bitcoin block for testing
pub fn create_test_block() -> Block {
    use bitcoin::{Block, BlockHash, CompactTarget, block::Header};
    use std::str::FromStr;

    Block {
        header: Header {
            version: bitcoin::block::Version::TWO,
            prev_blockhash: BlockHash::from_str(
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            merkle_root: bitcoin::TxMerkleNode::from_str(
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            time: 1234567890,
            bits: CompactTarget::from_consensus(0x1d00ffff),
            nonce: 0,
        },
        txdata: vec![create_test_transaction()],
    }
}
