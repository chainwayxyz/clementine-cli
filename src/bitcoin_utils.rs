// Bitcoin utility functions for Clementine CLI

use crate::config::{BridgeCliConfig, UNSPENDABLE_XONLY_PUBKEY};
use crate::errors::BridgeCliError;
use crate::script::{deposit_script, recover_script};
use crate::structs::SecureKeypair;
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
pub const WITHDRAWAL_UTXO_AMOUNT: Amount = Amount::from_sat(330);
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
/// Sign a recovery transaction with a given keypair, Citrea address, recovery taproot address, deposit outpoint, deposit amount, claim address, fee rate, and network
pub(crate) fn sign_recovery_tx(
    keypair: &SecureKeypair,
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &BitcoinAddress,
    deposit_outpoint: &OutPoint,
    deposit_amount: Amount,
    claim_address: &BitcoinAddress,
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
        script_pubkey: claim_address.script_pubkey(),
    };

    let mut recovery_tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![txin],
        output: vec![txout],
    };

    let weight = Weight::from_wu(550);
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
    claim_address: &BitcoinAddress,
    amount: Amount,
) -> Result<bitcoin::taproot::Signature, BridgeCliError> {
    let withdrawal_tx = create_withdrawal_transaction(withdrawal_utxo, claim_address, amount);
    let prevout = create_withdrawal_prevout(signer_address);

    let sighash = create_withdrawal_sighash(&withdrawal_tx, &prevout)?;
    let sig = sign_with_tweak(keypair, sighash, None);

    Ok(bitcoin::taproot::Signature {
        signature: sig,
        sighash_type: bitcoin::TapSighashType::SinglePlusAnyoneCanPay,
    })
}

fn create_withdrawal_transaction(
    withdrawal_utxo: &OutPoint,
    claim_address: &BitcoinAddress,
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
        script_pubkey: claim_address.script_pubkey(),
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
    claim_address: &BitcoinAddress,
    amount: Amount,
) -> Result<(), BridgeCliError> {
    let withdrawal_tx = create_withdrawal_transaction(withdrawal_utxo, claim_address, amount);
    let prevout = create_withdrawal_prevout(signer_address);

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
fn create_withdrawal_prevout(signer_address: &BitcoinAddress) -> TxOut {
    TxOut {
        value: WITHDRAWAL_UTXO_AMOUNT,
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
    use crate::wallet::address::calculate_taproot_address;

    use super::*;
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
}
