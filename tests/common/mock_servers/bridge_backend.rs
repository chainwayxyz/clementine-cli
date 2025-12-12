//! Mock helpers for Bridge Backend API

use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mock successful deposit account creation
/// POST /deposit-accounts
pub async fn mock_create_deposit_account_success(
    mock_server: &MockServer,
    taproot_address: &str,
    evm_address: &str,
) {
    Mock::given(method("POST"))
        .and(path("/deposit-accounts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "taproot_addr": taproot_address,
            "evm_addr": evm_address
        })))
        .mount(mock_server)
        .await;
}

/// Mock deposit account creation with specific request body validation
pub async fn mock_create_deposit_account_with_validation(
    mock_server: &MockServer,
    expected_evm_addr: &str,
    expected_recovery_addr: &str,
    response_taproot_addr: &str,
) {
    Mock::given(method("POST"))
        .and(path("/deposit-accounts"))
        .and(body_json(json!({
            "evm_addr": expected_evm_addr,
            "recovery_taproot_addr": expected_recovery_addr
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "taproot_addr": response_taproot_addr,
            "evm_addr": expected_evm_addr
        })))
        .mount(mock_server)
        .await;
}

/// Mock deposit account creation failure (500 error)
pub async fn mock_create_deposit_account_error(mock_server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/deposit-accounts"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal server error"))
        .mount(mock_server)
        .await;
}

/// Mock deposit account creation with invalid response (missing field)
pub async fn mock_create_deposit_account_invalid_response(mock_server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/deposit-accounts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "invalid_field": "value"
        })))
        .mount(mock_server)
        .await;
}

/// Mock successful deposit status request
/// GET /deposits?taproot_addrs=XXX
pub async fn mock_deposit_status_success(mock_server: &MockServer, status: &str) {
    Mock::given(method("GET"))
        .and(path("/deposits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": 1,
                "status": status,
                "txid": "1111111111111111111111111111111111111111111111111111111111111111",
                "evm_addr": "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb1",
                "move_tx_raw": "0x",
                "move_txid": "2222222222222222222222222222222222222222222222222222222222222222",
                "created_at": "2023-01-01T00:00:00Z",
                "mint_txid": "3333333333333333333333333333333333333333333333333333333333333333"
            }
        ])))
        .up_to_n_times(1)
        .mount(mock_server)
        .await;
}

/// Mock empty deposit status (no deposits found)
pub async fn mock_deposit_status_empty(mock_server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/deposits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .up_to_n_times(1)
        .mount(mock_server)
        .await;
}

/// Mock deposit status error
pub async fn mock_deposit_status_error(mock_server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/deposits"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Failed to fetch deposit status"))
        .up_to_n_times(1)
        .mount(mock_server)
        .await;
}

/// Mock successful withdrawal status request
/// GET /withdrawals?user_dust_outpoint=XXX
pub async fn mock_withdrawal_status_success(mock_server: &MockServer, status: &str) {
    Mock::given(method("GET"))
        .and(path("/withdrawals"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "idx": 1,
                "status": status,
                "btc_payment_txid": "2222222222222222222222222222222222222222222222222222222222222222",
                "from_safe_withdraw": true,
                "optimistic_payout_started_at": null,
                "optimistic_payout_deadline_at": null,
                "created_at": "2023-01-01T00:00:00Z",
                "optimistic_payout_payment": null
            }
        ])))
        .up_to_n_times(1)
        .mount(mock_server)
        .await;
}

/// Mock successful withdrawal signature submission
/// POST /withdrawals/user-signatures
pub async fn mock_send_withdrawal_signature_success(mock_server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/withdrawals/user-signatures"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "message": "Signature received"
        })))
        .up_to_n_times(1)
        .mount(mock_server)
        .await;
}

/// Mock withdrawal signature submission - withdrawal not found
pub async fn mock_send_withdrawal_signature_not_found(mock_server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/withdrawals/user-signatures"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Withdrawal not found"))
        .up_to_n_times(1)
        .mount(mock_server)
        .await;
}

/// Mock withdrawal signature submission - already exists
pub async fn mock_send_withdrawal_signature_already_exists(mock_server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/withdrawals/user-signatures"))
        .respond_with(
            ResponseTemplate::new(409).set_body_string("Withdrawal user signature already exists"),
        )
        .up_to_n_times(1)
        .mount(mock_server)
        .await;
}
