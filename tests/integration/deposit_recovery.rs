//! Integration tests for deposit recovery transaction workflows

use clementine_cli::__test_helpers::*;

use clementine_cli::__test_helpers::test_create_signed_recovery_tx;
use clementine_cli::errors::BridgeCliError;
use clementine_cli::wallet::Purpose;
use clementine_cli::{
    RecoveryTxParams, VerifyRecoveryTxParams, create_encrypted_wallet, verify_recovery_tx,
};
use serial_test::serial;

#[test]
#[serial]
fn test_create_recovery_tx_with_valid_params() {
    setup_integration_test_env();

    let network = test_network();

    // Create recovery wallet (needs Deposit purpose for recovery tx)
    let (recovery_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "recovery_wallet".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create recovery wallet");

    // Create destination wallet
    let (destination_address, _mnemonic2, _path2) = create_encrypted_wallet(
        network,
        "destination_wallet".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create destination wallet");

    // Load keypair from recovery wallet
    let keypair = test_load_key_with_purpose_check(&recovery_address, Purpose::Deposit).unwrap();

    // Prepare recovery transaction parameters
    let params = RecoveryTxParams {
        citrea_addr: test_citrea_address(),
        recovery_taproot_address: recovery_address,
        outpoint: test_outpoint(),
        destination_addr: destination_address.address.clone(),
        fee_rate: Some(10),  // 10 sat/vB
        amount: Some(0.001), // 0.001 BTC
    };

    let config = test_config();

    // Create recovery transaction
    let result = test_create_signed_recovery_tx(params, &config, keypair);

    // Verify transaction was created
    assert!(result.is_ok(), "Recovery transaction should be created");

    let recovery_tx = result.unwrap();

    // Verify transaction structure
    assert!(
        !recovery_tx.input.is_empty(),
        "Transaction should have inputs"
    );
    assert!(
        !recovery_tx.output.is_empty(),
        "Transaction should have outputs"
    );
}

#[test]
#[serial]
fn test_create_recovery_tx_with_different_fee_rates() {
    setup_integration_test_env();

    let network = test_network();

    // Create recovery wallet
    let (recovery_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "recovery_wallet_fees".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create recovery wallet");

    // Create destination wallet
    let (destination_address, _mnemonic2, _path2) = create_encrypted_wallet(
        network,
        "destination_wallet_fees".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create destination wallet");

    let keypair = test_load_key_with_purpose_check(&recovery_address, Purpose::Deposit).unwrap();
    let config = test_config();

    // Test with low fee rate
    let params_low_fee = RecoveryTxParams {
        citrea_addr: test_citrea_address(),
        recovery_taproot_address: recovery_address.clone(),
        outpoint: test_outpoint_with_index(0),
        destination_addr: destination_address.address.clone(),
        fee_rate: Some(1), // 1 sat/vB
        amount: Some(0.001),
    };

    let result_low = test_create_signed_recovery_tx(params_low_fee, &config, keypair);
    assert!(result_low.is_ok(), "Low fee transaction should succeed");

    // Test with high fee rate (reload keypair since SecureKeypair doesn't implement Clone)
    let keypair2 = load_test_keypair(&recovery_address);
    let params_high_fee = RecoveryTxParams {
        citrea_addr: test_citrea_address(),
        recovery_taproot_address: recovery_address,
        outpoint: test_outpoint_with_index(1),
        destination_addr: destination_address.address.clone(),
        fee_rate: Some(100), // 100 sat/vB
        amount: Some(0.001),
    };

    let result_high = test_create_signed_recovery_tx(params_high_fee, &config, keypair2);
    assert!(result_high.is_ok(), "High fee transaction should succeed");
}

#[test]
#[serial]
fn test_create_recovery_tx_with_different_amounts() {
    setup_integration_test_env();

    let network = test_network();

    // Create recovery wallet
    let (recovery_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "recovery_wallet_amounts".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create recovery wallet");

    // Create destination wallet
    let (destination_address, _mnemonic2, _path2) = create_encrypted_wallet(
        network,
        "destination_wallet_amounts".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create destination wallet");

    let config = test_config();

    // Test with specific amount
    let keypair1 = test_load_key_with_purpose_check(&recovery_address, Purpose::Deposit).unwrap();
    let params_with_amount = RecoveryTxParams {
        citrea_addr: test_citrea_address(),
        recovery_taproot_address: recovery_address.clone(),
        outpoint: test_outpoint_with_index(0),
        destination_addr: destination_address.address.clone(),
        fee_rate: Some(10),
        amount: Some(0.005), // 0.005 BTC
    };

    let result_with_amount = test_create_signed_recovery_tx(params_with_amount, &config, keypair1);
    assert!(
        result_with_amount.is_ok(),
        "Transaction with specific amount should succeed"
    );

    // Test with None (should use default/max amount)
    let keypair2 = load_test_keypair(&recovery_address);
    let params_no_amount = RecoveryTxParams {
        citrea_addr: test_citrea_address(),
        recovery_taproot_address: recovery_address,
        outpoint: test_outpoint_with_index(1),
        destination_addr: destination_address.address,
        fee_rate: Some(10),
        amount: None,
    };

    let result_no_amount = test_create_signed_recovery_tx(params_no_amount, &config, keypair2);
    assert!(
        result_no_amount.is_ok(),
        "Transaction without amount should succeed"
    );
}

#[test]
#[serial]
fn test_verify_recovery_tx_succeeds() {
    setup_integration_test_env();

    let network = test_network();

    // Create recovery wallet
    let (recovery_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "recovery_wallet_verify".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create recovery wallet");

    // Create destination wallet
    let (destination_address, _mnemonic2, _path2) = create_encrypted_wallet(
        network,
        "destination_wallet_verify".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create destination wallet");

    let keypair = test_load_key_with_purpose_check(&recovery_address, Purpose::Deposit).unwrap();
    let config = test_config();

    // Create a recovery transaction
    let params = RecoveryTxParams {
        citrea_addr: test_citrea_address(),
        recovery_taproot_address: recovery_address.clone(),
        outpoint: test_outpoint(),
        destination_addr: destination_address.address.clone(),
        fee_rate: Some(10),
        amount: Some(0.001),
    };

    let recovery_tx = test_create_signed_recovery_tx(params, &config, keypair)
        .expect("Should create recovery transaction");

    // Verify the transaction
    let verify_params = VerifyRecoveryTxParams {
        recovery_tx,
        citrea_address: test_citrea_address(),
        recovery_taproot_address: recovery_address,
        amount: Some(0.001),
    };

    let result = verify_recovery_tx(verify_params, &config);

    // Verification should succeed
    assert!(
        result.is_ok(),
        "Recovery transaction verification should succeed"
    );

    let (_txid, _address, _amount) = result.unwrap();
    // Transaction was verified successfully
}

#[test]
#[serial]
fn test_recovery_tx_determinism() {
    setup_integration_test_env();

    let network = test_network();

    // Create recovery wallet
    let (recovery_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "recovery_wallet_determinism".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create recovery wallet");

    // Create destination wallet
    let (destination_address, _mnemonic2, _path2) = create_encrypted_wallet(
        network,
        "destination_wallet_determinism".to_string(),
        Purpose::Deposit,
        test_passphrase(),
    )
    .expect("Should create destination wallet");

    let config = test_config();

    // Create first transaction
    let keypair1 = test_load_key_with_purpose_check(&recovery_address, Purpose::Deposit).unwrap();
    let params1 = RecoveryTxParams {
        citrea_addr: test_citrea_address(),
        recovery_taproot_address: recovery_address.clone(),
        outpoint: test_outpoint(),
        destination_addr: destination_address.address.clone(),
        fee_rate: Some(10),
        amount: Some(0.001),
    };

    let tx1 = test_create_signed_recovery_tx(params1, &config, keypair1)
        .expect("Should create first transaction");

    // Create second transaction with same parameters
    let keypair2 = load_test_keypair(&recovery_address);
    let params2 = RecoveryTxParams {
        citrea_addr: test_citrea_address(),
        recovery_taproot_address: recovery_address,
        outpoint: test_outpoint(),
        destination_addr: destination_address.address,
        fee_rate: Some(10),
        amount: Some(0.001),
    };

    let tx2 = test_create_signed_recovery_tx(params2, &config, keypair2)
        .expect("Should create second transaction");

    // Transactions should be identical (deterministic signing)
    assert_eq!(
        tx1.compute_txid(),
        tx2.compute_txid(),
        "Recovery transactions with same params should have same txid"
    );
}
