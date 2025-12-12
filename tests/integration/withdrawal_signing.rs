//! Integration tests for withdrawal signature generation

use clementine_cli::__test_helpers::*;
use clementine_cli::__test_helpers::{
    test_generate_withdrawal_signatures, test_get_private_key_from_wallet,
    test_secret_key_to_keypair,
};
use clementine_cli::create_encrypted_wallet;
use clementine_cli::wallet::Purpose;
use serial_test::serial;
use std::str::FromStr;

#[test]
#[serial]
fn test_generate_withdrawal_signatures_success() {
    setup_integration_test_env();
    let network = test_network();
    let config = test_config();

    // Create withdrawal wallet
    let (signer_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "withdrawal_sig_success".to_string(),
        Purpose::Withdrawal,
        test_passphrase(),
    )
    .expect("Should create withdrawal wallet");

    // Get private key from wallet and convert to keypair
    let private_key = test_get_private_key_from_wallet(&signer_address, &test_passphrase())
        .expect("Should get private key from wallet");
    let keypair = test_secret_key_to_keypair(&private_key);

    let destination_address = test_non_wallet_destination_address();

    let withdrawal_utxo = bitcoin::OutPoint::from_str(
        "5555555555555555555555555555555555555555555555555555555555555555:0",
    )
    .unwrap();

    let optimistic_amount = bitcoin::Amount::from_sat(100_000);
    let operator_amount = bitcoin::Amount::from_sat(90_000);

    let result = test_generate_withdrawal_signatures(
        keypair,
        &signer_address,
        &destination_address,
        &withdrawal_utxo,
        &optimistic_amount,
        &operator_amount,
        &config,
    );

    assert!(result.is_ok(), "Should generate signatures successfully");
    let (optimistic_sig, operator_sig) = result.unwrap();

    // Verify signatures are not empty
    assert!(!optimistic_sig.serialize().is_empty());
    assert!(!operator_sig.serialize().is_empty());
}

#[test]
#[serial]
fn test_generate_withdrawal_signatures_deterministic() {
    setup_integration_test_env();
    let network = test_network();
    let config = test_config();

    // Create withdrawal wallet
    let (signer_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "withdrawal_sig_deterministic".to_string(),
        Purpose::Withdrawal,
        test_passphrase(),
    )
    .expect("Should create withdrawal wallet");

    // Get private key from wallet and convert to keypair
    let private_key = test_get_private_key_from_wallet(&signer_address, &test_passphrase())
        .expect("Should get private key from wallet");
    let keypair1 = test_secret_key_to_keypair(&private_key);

    let destination_address = test_non_wallet_destination_address();

    let withdrawal_utxo = bitcoin::OutPoint::from_str(
        "7777777777777777777777777777777777777777777777777777777777777777:0",
    )
    .unwrap();

    let optimistic_amount = bitcoin::Amount::from_sat(100_000);
    let operator_amount = bitcoin::Amount::from_sat(90_000);

    let result1 = test_generate_withdrawal_signatures(
        keypair1,
        &signer_address,
        &destination_address,
        &withdrawal_utxo,
        &optimistic_amount,
        &operator_amount,
        &config,
    );

    // Get keypair again for second call
    let private_key = test_get_private_key_from_wallet(&signer_address, &test_passphrase())
        .expect("Should get private key from wallet");
    let keypair2 = test_secret_key_to_keypair(&private_key);

    let result2 = test_generate_withdrawal_signatures(
        keypair2,
        &signer_address,
        &destination_address,
        &withdrawal_utxo,
        &optimistic_amount,
        &operator_amount,
        &config,
    );

    let (opt_sig1, op_sig1) = result1.unwrap();
    let (opt_sig2, op_sig2) = result2.unwrap();

    assert_eq!(
        opt_sig1, opt_sig2,
        "Optimistic signatures should be deterministic"
    );
    assert_eq!(
        op_sig1, op_sig2,
        "Operator signatures should be deterministic"
    );
}

#[test]
#[serial]
fn test_generate_withdrawal_signatures_different_inputs() {
    setup_integration_test_env();
    let network = test_network();
    let config = test_config();

    // Create withdrawal wallet
    let (signer_address, _mnemonic, _path) = create_encrypted_wallet(
        network,
        "withdrawal_sig_different".to_string(),
        Purpose::Withdrawal,
        test_passphrase(),
    )
    .expect("Should create withdrawal wallet");

    // Get private key from wallet and convert to keypair
    let private_key = test_get_private_key_from_wallet(&signer_address, &test_passphrase())
        .expect("Should get private key from wallet");
    let keypair1 = test_secret_key_to_keypair(&private_key);

    let destination_address = test_non_wallet_destination_address();

    let withdrawal_utxo1 = bitcoin::OutPoint::from_str(
        "8888888888888888888888888888888888888888888888888888888888888888:0",
    )
    .unwrap();

    let withdrawal_utxo2 = bitcoin::OutPoint::from_str(
        "9999999999999999999999999999999999999999999999999999999999999999:0",
    )
    .unwrap();

    let optimistic_amount = bitcoin::Amount::from_sat(100_000);
    let operator_amount = bitcoin::Amount::from_sat(90_000);

    let result1 = test_generate_withdrawal_signatures(
        keypair1,
        &signer_address,
        &destination_address,
        &withdrawal_utxo1,
        &optimistic_amount,
        &operator_amount,
        &config,
    );

    // Get keypair again for second call
    let private_key = test_get_private_key_from_wallet(&signer_address, &test_passphrase())
        .expect("Should get private key from wallet");
    let keypair2 = test_secret_key_to_keypair(&private_key);

    let result2 = test_generate_withdrawal_signatures(
        keypair2,
        &signer_address,
        &destination_address,
        &withdrawal_utxo2, // Different UTXO
        &optimistic_amount,
        &operator_amount,
        &config,
    );

    let (opt_sig1, op_sig1) = result1.unwrap();
    let (opt_sig2, op_sig2) = result2.unwrap();

    assert_ne!(
        opt_sig1, opt_sig2,
        "Optimistic signatures should differ for different UTXOs"
    );
    assert_ne!(
        op_sig1, op_sig2,
        "Operator signatures should differ for different UTXOs"
    );
}
