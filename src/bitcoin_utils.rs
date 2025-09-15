// Bitcoin utility functions for Clementine CLI

use crate::config::{BridgeCliConfig, UNSPENDABLE_XONLY_PUBKEY};
use crate::errors::BridgeCliError;
use crate::script::{deposit_script, recover_script};
use crate::secure_types::SecureKeypair;
use crate::{BitcoinAddress, CitreaAddress};
use bitcoin::hashes::Hash;
use bitcoin::secp256k1::{Secp256k1, schnorr};
use bitcoin::taproot::{LeafVersion, TaprootBuilder, TaprootSpendInfo};
use bitcoin::{
    Amount, FeeRate, OutPoint, ScriptBuf, Sequence, TapLeafHash, TapNodeHash, TapSighash,
    TapTweakHash, Transaction, TxIn, TxOut, Txid, Weight, Witness, XOnlyPublicKey,
};
use eyre::{Context, Result};
use std::sync::LazyLock;

pub static SECP: LazyLock<Secp256k1<bitcoin::secp256k1::All>> = LazyLock::new(Secp256k1::new);

// Constants to reduce magic number duplication
pub const SATS_TO_WEI_MULTIPLIER: u64 = 10_000_000_000;

/// Convert optional BTC amount to optional Amount (reduces duplication)
pub fn convert_btc_to_amount(btc_amount: Option<f64>) -> Result<Option<Amount>, BridgeCliError> {
    match btc_amount {
        Some(btc) => Ok(Some(Amount::from_btc(btc)?)),
        None => Ok(None),
    }
}

/// Calculate the deposit address and taproot spend info for a given Citrea address and recovery taproot address
pub(crate) fn calculate_deposit_address(
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<(BitcoinAddress, TaprootSpendInfo), BridgeCliError> {
    let deposit_script = deposit_script(*citrea_address, config.aggregated_public_key);
    let recovery_key = extract_xonly_pubkey_from_address(recovery_taproot_address)?;
    let recover_script = recover_script(recovery_key, config.user_takes_after);

    let taproot_spend_info = TaprootBuilder::new()
        .add_leaf(1, deposit_script)
        .expect("deposit script is valid")
        .add_leaf(1, recover_script)
        .expect("recover script is valid")
        .finalize(&SECP, *UNSPENDABLE_XONLY_PUBKEY)
        .expect("finalized script is valid");

    let deposit_address = BitcoinAddress::p2tr(
        &SECP,
        *UNSPENDABLE_XONLY_PUBKEY,
        taproot_spend_info.merkle_root(),
        config.network,
    );
    Ok((deposit_address, taproot_spend_info))
}

fn sign_with_tweak(
    keypair: &SecureKeypair,
    sighash: TapSighash,
    merkle_root: Option<TapNodeHash>,
) -> schnorr::Signature {
    use bitcoin::hashes::Hash;
    SECP.sign_schnorr(
        &bitcoin::secp256k1::Message::from_digest(*sighash.as_byte_array()),
        &keypair
            .as_ref()
            .add_xonly_tweak(
                &SECP,
                &TapTweakHash::from_key_and_tweak(
                    keypair.as_ref().x_only_public_key().0,
                    merkle_root,
                )
                .to_scalar(),
            )
            .unwrap(),
    )
}

#[allow(clippy::too_many_arguments)]
/// Sign a recovery transaction with a given keypair, Citrea address, recovery taproot address, deposit outpoint, deposit amount, destination address, fee rate, and network
pub(crate) fn sign_recovery_tx(
    keypair: &SecureKeypair,
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &BitcoinAddress,
    deposit_outpoint: &OutPoint,
    deposit_amount: Amount,
    destination_address: &BitcoinAddress,
    fee_rate: FeeRate,
    config: &BridgeCliConfig,
) -> Result<Transaction, BridgeCliError> {
    let (deposit_address, taproot_spend_info) =
        calculate_deposit_address(citrea_address, recovery_taproot_address, config)?;

    let recovery_script =
        create_recovery_script_for_address(recovery_taproot_address, config.user_takes_after)?;

    let txin = TxIn {
        previous_output: *deposit_outpoint,
        script_sig: ScriptBuf::default(),
        sequence: Sequence::from_height(config.user_takes_after as u16),
        witness: Witness::default(),
    };

    let prevout = TxOut {
        value: deposit_amount,
        script_pubkey: deposit_address.script_pubkey(),
    };

    let txout = TxOut {
        value: deposit_amount,
        script_pubkey: destination_address.script_pubkey(),
    };

    let mut recovery_tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![txin],
        output: vec![txout],
    };

    // This is the weight of the transaction without script_pubkey.
    let mut weight = Weight::from_wu(414);
    let destination_address_script_len = destination_address.script_pubkey().to_bytes().len();
    weight += Weight::from_wu(destination_address_script_len as u64 * 4);
    let fee = fee_rate.fee_wu(weight).expect("fee is valid");
    let output_amount: Amount = match deposit_amount.checked_sub(fee) {
        Some(amt) => amt,
        None => return Err(eyre::eyre!("Insufficient funds for fee").into()),
    };
    if output_amount < Amount::from_sat(546) {
        return Err(eyre::eyre!("Output amount below dust threshold").into());
    }
    recovery_tx.output[0].value = output_amount;

    let mut sighash_cache = bitcoin::sighash::SighashCache::new(recovery_tx.clone());

    let sighash = sighash_cache
        .taproot_script_spend_signature_hash(
            0,
            &bitcoin::sighash::Prevouts::All(&[prevout]),
            TapLeafHash::from_script(&recovery_script, LeafVersion::TapScript),
            bitcoin::TapSighashType::Default,
        )
        .unwrap();

    tracing::debug!("sighash: {:?}", sighash);
    tracing::debug!("recovery_script: {:?}", recovery_script);
    tracing::debug!(
        "recovery key: {:?}",
        extract_xonly_pubkey_from_address(recovery_taproot_address)
    );
    tracing::debug!("input_amount: {:?}", deposit_amount);

    let sig = sign_with_tweak(keypair, sighash, None);

    let taproot_signature = bitcoin::taproot::Signature {
        signature: sig,
        sighash_type: bitcoin::TapSighashType::Default,
    };

    let spend_control_block = taproot_spend_info
        .control_block(&(recovery_script.clone(), LeafVersion::TapScript))
        .unwrap();

    let mut witness = bitcoin::Witness::new();
    witness.push(taproot_signature.serialize());
    witness.push(recovery_script.as_script());
    witness.push(spend_control_block.serialize());

    recovery_tx.input[0].witness = witness;

    tracing::debug!(
        "recovery_tx: {:?}",
        hex::encode(bitcoin::consensus::serialize(&recovery_tx))
    );
    let weight = recovery_tx.weight();
    tracing::debug!("weight: {:?}", weight);

    Ok(recovery_tx)
}

pub(crate) fn verify_recovery_tx(
    recovery_tx: &Transaction,
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &BitcoinAddress,
    input_amount: Option<Amount>,
    config: &BridgeCliConfig,
) -> Result<(Txid, BitcoinAddress, Amount), BridgeCliError> {
    // sanity check input count
    if recovery_tx.input.len() != 1 {
        return Err(eyre::eyre!("Recovery transaction must have exactly one input").into());
    }

    // sanity check output count
    if recovery_tx.output.len() != 1 {
        return Err(eyre::eyre!("Recovery transaction must have exactly one output").into());
    }

    // sanity check that the input has a witness
    if recovery_tx.input[0].witness.is_empty() {
        return Err(eyre::eyre!("Recovery transaction input must have a witness").into());
    }

    // sanity check that the witness has 3 items
    if recovery_tx.input[0].witness.len() != 3 {
        return Err(
            eyre::eyre!("Recovery transaction input witness must have exactly 3 items").into(),
        );
    }

    let (deposit_address, taproot_spend_info) =
        calculate_deposit_address(citrea_address, recovery_taproot_address, config)?;

    let recovery_script =
        create_recovery_script_for_address(recovery_taproot_address, config.user_takes_after)?;
    let recovery_key = extract_xonly_pubkey_from_address(recovery_taproot_address)?;

    // 1. check that the second element of the witness is the recovery script
    if recovery_tx.input[0].witness[1] != recovery_script.as_script().to_bytes() {
        return Err(eyre::eyre!(
            "Recovery transaction input witness second element is not the correct recovery script, may be a different recovery script"
        ).into());
    }

    // 2. check that the third element of the witness is the spend control block
    if recovery_tx.input[0].witness[2]
        != taproot_spend_info
            .control_block(&(recovery_script.clone(), LeafVersion::TapScript))
            .unwrap()
            .serialize()
    {
        return Err(eyre::eyre!(
            "Recovery transaction input witness third element is not the correct spend control block, may be a different spend control block"
        ).into());
    }

    let taproot_signature =
        bitcoin::taproot::Signature::from_slice(&recovery_tx.input[0].witness[0]).wrap_err(
            "Recovery transaction input witness first element is not a valid taproot signature",
        )?;

    let sighash_type =
        if taproot_signature.sighash_type == bitcoin::TapSighashType::SinglePlusAnyoneCanPay {
            bitcoin::TapSighashType::SinglePlusAnyoneCanPay
        } else if taproot_signature.sighash_type == bitcoin::TapSighashType::Default
            || taproot_signature.sighash_type == bitcoin::TapSighashType::All
        {
            bitcoin::TapSighashType::Default
        } else {
            return Err(eyre::eyre!("Signature type not supported").into());
        };

    let input_amount = input_amount.unwrap_or(config.bridge_amount);

    let prevout = TxOut {
        value: input_amount,
        script_pubkey: deposit_address.script_pubkey(),
    };

    let mut sighash_cache = bitcoin::sighash::SighashCache::new(recovery_tx.clone());

    let sighash = if sighash_type == bitcoin::TapSighashType::SinglePlusAnyoneCanPay {
        sighash_cache
            .taproot_script_spend_signature_hash(
                0,
                &bitcoin::sighash::Prevouts::One(0, &prevout),
                TapLeafHash::from_script(&recovery_script, LeafVersion::TapScript),
                bitcoin::TapSighashType::SinglePlusAnyoneCanPay,
            )
            .unwrap()
    } else {
        sighash_cache
            .taproot_script_spend_signature_hash(
                0,
                &bitcoin::sighash::Prevouts::All(&[prevout]),
                TapLeafHash::from_script(&recovery_script, LeafVersion::TapScript),
                bitcoin::TapSighashType::Default,
            )
            .unwrap()
    };

    tracing::debug!("sighash: {:?}", sighash);
    tracing::debug!("taproot_signature: {:?}", taproot_signature);
    tracing::debug!("recovery_key: {:?}", recovery_key);
    tracing::debug!("input_amount: {:?}", input_amount);

    // verify the signature
    SECP.verify_schnorr(
        &taproot_signature.signature,
        &bitcoin::secp256k1::Message::from_digest(*sighash.as_byte_array()),
        &recovery_key,
    )
    .wrap_err(
        "Signature verification failed. Possible causes include an incorrect input amount, an invalid signature, or a mismatched public key."
    )?;

    let output_address = BitcoinAddress::from_script(
        &recovery_tx.output[0].script_pubkey,
        config.network,
    )
    .wrap_err(
        "Recovery transaction output script pubkey is not a valid address, may be a different address"
    )?;

    Ok((
        recovery_tx.input[0].previous_output.txid,
        output_address,
        recovery_tx.output[0].value,
    ))
}

pub(crate) fn sign_withdrawal_signature(
    keypair: &SecureKeypair,
    signer_address: &BitcoinAddress,
    withdrawal_utxo: &OutPoint,
    destination_address: &BitcoinAddress,
    amount: Amount,
    config: &BridgeCliConfig,
) -> Result<bitcoin::taproot::Signature, BridgeCliError> {
    let withdrawal_tx = create_withdrawal_transaction(withdrawal_utxo, destination_address, amount);
    let prevout = create_withdrawal_prevout(signer_address, config);

    let sighash = create_withdrawal_sighash(&withdrawal_tx, &prevout)?;
    let sig = sign_with_tweak(keypair, sighash, None);

    Ok(bitcoin::taproot::Signature {
        signature: sig,
        sighash_type: bitcoin::TapSighashType::SinglePlusAnyoneCanPay,
    })
}

fn create_withdrawal_transaction(
    withdrawal_utxo: &OutPoint,
    destination_address: &BitcoinAddress,
    amount: Amount,
) -> Transaction {
    let txin = TxIn {
        previous_output: *withdrawal_utxo,
        script_sig: ScriptBuf::default(),
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        witness: Witness::default(),
    };

    let txout = TxOut {
        value: amount,
        script_pubkey: destination_address.script_pubkey(),
    };

    Transaction {
        version: bitcoin::transaction::Version::non_standard(3),
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![txin],
        output: vec![txout],
    }
}

pub(crate) fn verify_withdrawal_signature(
    sig: &bitcoin::taproot::Signature,
    signer_address: &BitcoinAddress,
    withdrawal_utxo: &OutPoint,
    destination_address: &BitcoinAddress,
    amount: Amount,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    let withdrawal_tx = create_withdrawal_transaction(withdrawal_utxo, destination_address, amount);
    let prevout = create_withdrawal_prevout(signer_address, config);

    let sighash = create_withdrawal_sighash(&withdrawal_tx, &prevout)?;

    SECP.verify_schnorr(
        &sig.signature,
        &bitcoin::secp256k1::Message::from_digest(*sighash.as_byte_array()),
        &extract_xonly_pubkey_from_address(signer_address)?,
    )
    .wrap_err("Signature verification failed")?;

    Ok(())
}

/// Create a sighash for withdrawal transactions (reduces duplication)
fn create_withdrawal_sighash(
    withdrawal_tx: &Transaction,
    prevout: &TxOut,
) -> Result<TapSighash, BridgeCliError> {
    let mut sighash_cache = bitcoin::sighash::SighashCache::new(withdrawal_tx.clone());
    Ok(sighash_cache
        .taproot_key_spend_signature_hash(
            0,
            &bitcoin::sighash::Prevouts::One(0, prevout),
            bitcoin::TapSighashType::SinglePlusAnyoneCanPay,
        )
        .unwrap())
}

/// Create prevout for withdrawal transactions (reduces duplication)
fn create_withdrawal_prevout(signer_address: &BitcoinAddress, config: &BridgeCliConfig) -> TxOut {
    TxOut {
        value: config.dust_utxo_amount,
        script_pubkey: signer_address.script_pubkey(),
    }
}

/// Extract XOnly public key from taproot address (reduces duplication)
fn extract_xonly_pubkey_from_address(
    address: &BitcoinAddress,
) -> Result<XOnlyPublicKey, BridgeCliError> {
    XOnlyPublicKey::from_slice(&address.script_pubkey().to_bytes()[2..34])
        .map_err(|e| eyre::eyre!("Failed to extract XOnly public key: {e}").into())
}

/// Create recovery script from taproot address (reduces duplication)
fn create_recovery_script_for_address(
    recovery_taproot_address: &BitcoinAddress,
    user_takes_after: u64,
) -> Result<ScriptBuf, BridgeCliError> {
    let recovery_key = extract_xonly_pubkey_from_address(recovery_taproot_address)?;
    Ok(recover_script(recovery_key, user_takes_after))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::address::calculate_taproot_address;
    use bitcoin::Network;
    use bitcoin::address::AddressType;
    use bitcoin::key::Keypair;
    use bitcoin::secp256k1::SecretKey;

    #[test]
    fn test_calculate_taproot_address() {
        let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
        let keypair = Keypair::from_secret_key(&SECP, &secret_key);
        let secure_keypair = SecureKeypair::new(keypair);
        let address = calculate_taproot_address(&secure_keypair, Network::Testnet4);
        assert_eq!(address.address_type(), Some(AddressType::P2tr));
    }

    #[test]
    fn test_deposit_amount_hex() {
        let deposit_amount_u64: u64 = 10_000_000_000_000_000_000;
        let deposit_amount_hex = format!("0x{:X}", deposit_amount_u64);
        assert_eq!(deposit_amount_hex, "0x8AC7230489E80000");
    }

    fn create_test_config() -> BridgeCliConfig {
        use crate::config::{BridgeCliConfig, NetworkConfigs};

        // Use default regtest config from NetworkConfigs
        let network_configs = NetworkConfigs {
            bitcoin: BridgeCliConfig::default(),
            testnet4: BridgeCliConfig::default(),
            signet: BridgeCliConfig::default(),
            regtest: BridgeCliConfig {
                network: Network::Regtest,
                aggregated_public_key: *crate::config::UNSPENDABLE_XONLY_PUBKEY,
                mempool_api_url: reqwest::Url::parse("http://localhost:3006").unwrap(),
                citrea_chain_id: 5115,
                citrea_rpc_url: reqwest::Url::parse("http://localhost:8545").unwrap(),
                citrea_backend_endpoint: reqwest::Url::parse("http://localhost:8080").unwrap(),
                user_takes_after: 4320,
                bridge_amount: Amount::from_sat(100000),
                optimistic_withdrawal_amount: Amount::from_sat(50000),
                operator_withdrawal_amount: Amount::from_sat(1000),
                dust_utxo_amount: Amount::from_sat(330),
                bridge_contract_address: "0x1234567890123456789012345678901234567890".to_string(),
                bitcoin_config: None,
            },
        };

        network_configs.regtest
    }

    fn create_test_keypair() -> SecureKeypair {
        let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
        let keypair = Keypair::from_secret_key(&SECP, &secret_key);
        SecureKeypair::new(keypair)
    }

    fn create_test_outpoint() -> OutPoint {
        use bitcoin::Txid;
        OutPoint {
            txid: Txid::from_byte_array([1u8; 32]),
            vout: 0,
        }
    }

    struct TestSetup {
        config: BridgeCliConfig,
        recovery_keypair: SecureKeypair,
        recovery_address: BitcoinAddress,
        citrea_address: CitreaAddress,
        deposit_outpoint: OutPoint,
        deposit_amount: Amount,
    }

    impl TestSetup {
        fn new() -> Self {
            let config = create_test_config();
            let recovery_keypair = create_test_keypair();
            let recovery_address = calculate_taproot_address(&recovery_keypair, config.network);
            let citrea_address = CitreaAddress::from([0u8; 20]);
            let deposit_outpoint = create_test_outpoint();
            let deposit_amount = Amount::from_sat(100000);

            Self {
                config,
                recovery_keypair,
                recovery_address,
                citrea_address,
                deposit_outpoint,
                deposit_amount,
            }
        }
    }

    fn create_destination_address(
        address_type: AddressType,
        network: Network,
        key_offset: u8,
    ) -> BitcoinAddress {
        use bitcoin::key::{CompressedPublicKey, PublicKey};
        use bitcoin::script::Builder;

        let destination_secret = SecretKey::from_slice(&[key_offset; 32]).unwrap();
        let destination_keypair = Keypair::from_secret_key(&SECP, &destination_secret);
        let destination_pubkey = PublicKey::from(destination_keypair.public_key());
        let destination_compressed_pubkey =
            CompressedPublicKey::try_from(destination_pubkey).unwrap();

        match address_type {
            AddressType::P2tr => {
                let secure_keypair = SecureKeypair::new(destination_keypair);
                calculate_taproot_address(&secure_keypair, network)
            }
            AddressType::P2wpkh => BitcoinAddress::p2wpkh(&destination_compressed_pubkey, network),
            AddressType::P2pkh => BitcoinAddress::p2pkh(destination_compressed_pubkey, network),
            AddressType::P2sh => {
                let destination_redeem_script = Builder::new()
                    .push_int(0)
                    .push_slice(destination_compressed_pubkey.pubkey_hash())
                    .into_script();
                BitcoinAddress::p2sh(&destination_redeem_script, network).unwrap()
            }
            AddressType::P2wsh => {
                let witness_script = Builder::new()
                    .push_slice(destination_compressed_pubkey.to_bytes())
                    .push_opcode(bitcoin::opcodes::all::OP_CHECKSIG)
                    .into_script();
                BitcoinAddress::p2wsh(&witness_script, network)
            }
            _ => panic!("Unsupported address type"),
        }
    }

    fn get_test_fee_rates() -> [FeeRate; 4] {
        [
            FeeRate::from_sat_per_vb(1).unwrap(),
            FeeRate::from_sat_per_vb(10).unwrap(),
            FeeRate::from_sat_per_vb(50).unwrap(),
            FeeRate::from_sat_per_vb(100).unwrap(),
        ]
    }

    fn assert_fee_rate_correctness(
        signed_tx: &Transaction,
        fee_rate: FeeRate,
        deposit_amount: Amount,
    ) {
        assert_eq!(
            signed_tx.input.len(),
            1,
            "Transaction must have exactly one input"
        );
        assert_eq!(
            signed_tx.output.len(),
            1,
            "Transaction must have exactly one output"
        );

        let actual_weight = signed_tx.weight();
        let expected_fee = fee_rate.fee_wu(actual_weight).unwrap();
        let expected_output_amount = deposit_amount.checked_sub(expected_fee).unwrap();

        assert_eq!(
            signed_tx.output[0].value, expected_output_amount,
            "Output amount should account for fees correctly"
        );

        let actual_fee = deposit_amount - signed_tx.output[0].value;
        assert_eq!(
            actual_fee, expected_fee,
            "Fee calculation should be correct"
        );

        assert!(
            signed_tx.output[0].value >= Amount::from_sat(546),
            "Output should be above dust threshold"
        );
    }

    fn test_fee_rate_correctness_for_address_type(address_type: AddressType, key_offset: u8) {
        let setup = TestSetup::new();
        let destination_address =
            create_destination_address(address_type, setup.config.network, key_offset);

        for fee_rate in get_test_fee_rates() {
            let result = sign_recovery_tx(
                &setup.recovery_keypair,
                &setup.citrea_address,
                &setup.recovery_address,
                &setup.deposit_outpoint,
                setup.deposit_amount,
                &destination_address,
                fee_rate,
                &setup.config,
            );

            assert!(
                result.is_ok(),
                "Failed to sign recovery tx with fee rate {:?} for address type",
                fee_rate
            );

            let signed_tx = result.unwrap();
            assert_fee_rate_correctness(&signed_tx, fee_rate, setup.deposit_amount);
        }
    }

    #[test]
    fn test_sign_recovery_tx_p2tr_fee_rate_correctness() {
        test_fee_rate_correctness_for_address_type(AddressType::P2tr, 2);
    }

    #[test]
    fn test_sign_recovery_tx_p2wpkh_fee_rate_correctness() {
        test_fee_rate_correctness_for_address_type(AddressType::P2wpkh, 3);
    }

    #[test]
    fn test_sign_recovery_tx_p2pkh_fee_rate_correctness() {
        test_fee_rate_correctness_for_address_type(AddressType::P2pkh, 4);
    }

    #[test]
    fn test_sign_recovery_tx_p2sh_fee_rate_correctness() {
        test_fee_rate_correctness_for_address_type(AddressType::P2sh, 5);
    }

    #[test]
    fn test_sign_recovery_tx_p2wsh_fee_rate_correctness() {
        test_fee_rate_correctness_for_address_type(AddressType::P2wsh, 6);
    }
}
