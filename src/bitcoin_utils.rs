// Bitcoin utility functions for Clementine CLI

use bitcoin::secp256k1::{Keypair, Secp256k1, SecretKey, schnorr};
use bitcoin::taproot::{LeafVersion, TaprootBuilder, TaprootSpendInfo};
use bitcoin::{
    Address, Amount, FeeRate, Network, OutPoint, ScriptBuf, Sequence, TapLeafHash, TapNodeHash,
    TapSighash, TapTweakHash, Transaction, TxIn, TxOut, Txid, Weight, Witness, XOnlyPublicKey,
};
use colored::*;
use std::io::{self, Write};
use std::str::FromStr;
use std::sync::LazyLock;

use crate::EVMAddress;
use crate::config::{BRIDGE_AMOUNT, CliConfig, UNSPENDABLE_XONLY_PUBKEY, USER_TAKES_AFTER};
use crate::musig2::AggregateFromPublicKeys;
use crate::script::{deposit_script, recover_script};
use bitcoin::hashes::Hash;

pub static SECP: LazyLock<Secp256k1<bitcoin::secp256k1::All>> = LazyLock::new(Secp256k1::new);

/// Calculate taproot address from a keypair
pub fn calculate_taproot_address(keypair: &Keypair, network: Network) -> Address {
    let (xonly_public_key, _parity) = keypair.public_key().x_only_public_key();
    Address::p2tr(&SECP, xonly_public_key, None, network)
}

/// Generate a new random secret key and calculate its corresponding taproot address
pub fn generate_key_and_taproot_address(
    network: Network,
) -> Result<(Keypair, Address), Box<dyn std::error::Error>> {
    let keypair = Keypair::new(&SECP, &mut bitcoin::secp256k1::rand::thread_rng());
    let address = calculate_taproot_address(&keypair, network);
    Ok((keypair, address))
}

pub fn generate_keypair_and_taproot_address_from_private_key(
    private_key: &str,
    network: Network,
) -> Result<(Keypair, Address), Box<dyn std::error::Error>> {
    let sk = SecretKey::from_str(private_key)?;
    let keypair = Keypair::from_secret_key(&SECP, &sk);
    let address = calculate_taproot_address(&keypair, network);
    Ok((keypair, address))
}

/// Prompt user for confirmation about storing private key
pub fn confirm_private_key_storage(auto_yes: bool) -> Result<bool, Box<dyn std::error::Error>> {
    if auto_yes {
        return Ok(true);
    }

    println!(
        "{} This command will save a private key to your computer.",
        "WARNING".red().bold()
    );
    println!("   Anyone with access to this computer could potentially spend your funds.");
    println!("   Make sure you're running this in a secure environment.");
    println!();
    print!("Are you sure you want to continue? (y/N): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().to_lowercase() == "y" || input.trim().to_lowercase() == "yes")
}

/// Calculate the deposit address and taproot spend info for a given EVM address and recovery taproot address
pub fn calculate_deposit_address(
    evm_address: &EVMAddress,
    recovery_taproot_address: &Address,
    config: CliConfig,
) -> Result<(Address, TaprootSpendInfo), Box<dyn std::error::Error>> {
    let agg_pk = XOnlyPublicKey::from_musig2_pks(config.verifiers_pks.as_slice())?;
    debug!("verifiers_public_keys: {:?}", config.verifiers_pks);
    debug!("agg_pk: {:?}", agg_pk.to_string());
    let deposit_script = deposit_script(*evm_address, agg_pk);
    let recovery_key =
        XOnlyPublicKey::from_slice(&recovery_taproot_address.script_pubkey().to_bytes()[2..34])?;
    let recover_script = recover_script(recovery_key, USER_TAKES_AFTER);

    let taproot_spend_info = TaprootBuilder::new()
        .add_leaf(1, deposit_script)
        .expect("deposit script is valid")
        .add_leaf(1, recover_script)
        .expect("recover script is valid")
        .finalize(&SECP, *UNSPENDABLE_XONLY_PUBKEY)
        .expect("finalized script is valid");

    let deposit_address = Address::p2tr(
        &SECP,
        *UNSPENDABLE_XONLY_PUBKEY,
        taproot_spend_info.merkle_root(),
        config.network,
    );
    Ok((deposit_address, taproot_spend_info))
}

/// Sign a taproot script spend with a given keypair and sighash
pub fn schnorr_sign(keypair: Keypair, sighash: TapSighash) -> schnorr::Signature {
    use bitcoin::hashes::Hash;
    SECP.sign_schnorr(
        &bitcoin::secp256k1::Message::from_digest(*sighash.as_byte_array()),
        &keypair,
    )
}

pub fn sign_with_tweak(
    keypair: Keypair,
    sighash: TapSighash,
    merkle_root: Option<TapNodeHash>,
) -> schnorr::Signature {
    use bitcoin::hashes::Hash;
    SECP.sign_schnorr(
        &bitcoin::secp256k1::Message::from_digest(*sighash.as_byte_array()),
        &keypair
            .add_xonly_tweak(
                &SECP,
                &TapTweakHash::from_key_and_tweak(keypair.x_only_public_key().0, merkle_root)
                    .to_scalar(),
            )
            .unwrap(),
    )
}

#[allow(clippy::too_many_arguments)]
/// Sign a recovery transaction with a given keypair, EVM address, recovery taproot address, deposit outpoint, deposit amount, claim address, fee rate, and network
pub fn sign_recovery_tx(
    keypair: &Keypair,
    evm_address: &EVMAddress,
    recovery_taproot_address: &Address,
    deposit_outpoint: &OutPoint,
    deposit_amount: Option<Amount>,
    claim_address: &Address,
    fee_rate: Option<FeeRate>,
    config: CliConfig,
) -> Result<Transaction, Box<dyn std::error::Error>> {
    let (deposit_address, taproot_spend_info) =
        calculate_deposit_address(evm_address, recovery_taproot_address, config)?;

    let recovery_script = recover_script(
        XOnlyPublicKey::from_slice(&recovery_taproot_address.script_pubkey().to_bytes()[2..34])?,
        USER_TAKES_AFTER,
    );

    let input_amount = deposit_amount.unwrap_or(BRIDGE_AMOUNT);

    let txin = TxIn {
        previous_output: *deposit_outpoint,
        script_sig: ScriptBuf::default(),
        sequence: Sequence::from_height(USER_TAKES_AFTER as u16),
        witness: Witness::default(),
    };

    let prevout = TxOut {
        value: input_amount,
        script_pubkey: deposit_address.script_pubkey(),
    };

    let txout = TxOut {
        value: input_amount,
        script_pubkey: claim_address.script_pubkey(),
    };

    let mut recovery_tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![txin],
        output: vec![txout],
    };

    if let Some(fee_rate) = fee_rate {
        let weight = Weight::from_wu(550);
        let fee = fee_rate.fee_wu(weight).expect("fee is valid");
        let output_amount: Amount = match input_amount.checked_sub(fee) {
            Some(amt) => amt,
            None => return Err("Insufficient funds for fee".into()),
        };
        if output_amount < Amount::from_sat(546) {
            return Err("Output amount below dust threshold".into());
        }
        recovery_tx.output[0].value = output_amount;
    }

    let mut sighash_cache = bitcoin::sighash::SighashCache::new(recovery_tx.clone());

    let sighash = if fee_rate.is_some() {
        sighash_cache
            .taproot_script_spend_signature_hash(
                0,
                &bitcoin::sighash::Prevouts::All(&[prevout]),
                TapLeafHash::from_script(&recovery_script, LeafVersion::TapScript),
                bitcoin::TapSighashType::Default,
            )
            .unwrap()
    } else {
        sighash_cache
            .taproot_script_spend_signature_hash(
                0,
                &bitcoin::sighash::Prevouts::One(0, &prevout),
                TapLeafHash::from_script(&recovery_script, LeafVersion::TapScript),
                bitcoin::TapSighashType::SinglePlusAnyoneCanPay,
            )
            .unwrap()
    };

    debug!("sighash: {:?}", sighash);
    debug!("keypair: {:?}", keypair);
    debug!("recovery_script: {:?}", recovery_script);
    debug!(
        "recovery key: {:?}",
        XOnlyPublicKey::from_slice(&recovery_taproot_address.script_pubkey().to_bytes()[2..34])
    );
    debug!("input_amount: {:?}", input_amount);

    let sig = sign_with_tweak(*keypair, sighash, None);

    let taproot_signature = bitcoin::taproot::Signature {
        signature: sig,
        sighash_type: if fee_rate.is_some() {
            bitcoin::TapSighashType::Default
        } else {
            bitcoin::TapSighashType::SinglePlusAnyoneCanPay
        },
    };

    let spend_control_block = taproot_spend_info
        .control_block(&(recovery_script.clone(), LeafVersion::TapScript))
        .unwrap();

    let mut witness = bitcoin::Witness::new();
    witness.push(taproot_signature.serialize());
    witness.push(recovery_script.as_script());
    witness.push(spend_control_block.serialize());

    recovery_tx.input[0].witness = witness;

    debug!(
        "recovery_tx: {:?}",
        hex::encode(bitcoin::consensus::serialize(&recovery_tx))
    );
    let weight = recovery_tx.weight();
    debug!("weight: {:?}", weight);

    Ok(recovery_tx)
}

pub fn verify_recovery_tx(
    recovery_tx: &Transaction,
    evm_address: &EVMAddress,
    recovery_taproot_address: &Address,
    input_amount: Option<Amount>,
    config: CliConfig,
) -> Result<(Txid, Address, Amount), Box<dyn std::error::Error>> {
    // sanity check input count
    if recovery_tx.input.len() != 1 {
        return Err("Recovery transaction must have exactly one input".into());
    }

    // sanity check output count
    if recovery_tx.output.len() != 1 {
        return Err("Recovery transaction must have exactly one output".into());
    }

    // sanity check that the input has a witness
    if recovery_tx.input[0].witness.is_empty() {
        return Err("Recovery transaction input must have a witness".into());
    }

    // sanity check that the witness has 3 items
    if recovery_tx.input[0].witness.len() != 3 {
        return Err("Recovery transaction input witness must have exactly 3 items".into());
    }

    let (deposit_address, taproot_spend_info) =
        calculate_deposit_address(evm_address, recovery_taproot_address, config.clone())?;

    let recovery_key =
        XOnlyPublicKey::from_slice(&recovery_taproot_address.script_pubkey().to_bytes()[2..34])?;

    let recovery_script = recover_script(recovery_key, USER_TAKES_AFTER);

    // 1. check that the second element of the witness is the recovery script
    if recovery_tx.input[0].witness[1] != recovery_script.as_script().to_bytes() {
        return Err("Recovery transaction input witness second element is not the correct recovery script, may be a different recovery script".into());
    }

    // 2. check that the third element of the witness is the spend control block
    if recovery_tx.input[0].witness[2]
        != taproot_spend_info
            .control_block(&(recovery_script.clone(), LeafVersion::TapScript))
            .unwrap()
            .serialize()
    {
        return Err("Recovery transaction input witness third element is not the correct spend control block, may be a different spend control block".into());
    }

    let taproot_signature = bitcoin::taproot::Signature::from_slice(
        &recovery_tx.input[0].witness[0],
    )
    .map_err(|_| -> Box<dyn std::error::Error> {
        "Recovery transaction input witness first element is not a valid taproot signature".into()
    })?;

    let sighash_type =
        if taproot_signature.sighash_type == bitcoin::TapSighashType::SinglePlusAnyoneCanPay {
            bitcoin::TapSighashType::SinglePlusAnyoneCanPay
        } else if taproot_signature.sighash_type == bitcoin::TapSighashType::Default
            || taproot_signature.sighash_type == bitcoin::TapSighashType::All
        {
            bitcoin::TapSighashType::Default
        } else {
            return Err("Signature type not supported".into());
        };

    let input_amount = input_amount.unwrap_or(BRIDGE_AMOUNT);

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

    debug!("sighash: {:?}", sighash);
    debug!("taproot_signature: {:?}", taproot_signature);
    debug!("recovery_key: {:?}", recovery_key);
    debug!("input_amount: {:?}", input_amount);

    // verify the signature
    SECP.verify_schnorr(
        &taproot_signature.signature,
        &bitcoin::secp256k1::Message::from_digest(*sighash.as_byte_array()),
        &recovery_key,
    )
    .map_err(|_| -> Box<dyn std::error::Error> {
        "Signature verification failed. Possible causes include an incorrect input amount, an invalid signature, or a mismatched public key.".into()
    })?;

    let output_address = Address::from_script(
        &recovery_tx.output[0].script_pubkey,
        config.network,
    )
    .map_err(|_| -> Box<dyn std::error::Error> {
        "Recovery transaction output script pubkey is not a valid address, may be a different address".into()
    })?;

    Ok((
        recovery_tx.input[0].previous_output.txid,
        output_address,
        recovery_tx.output[0].value,
    ))
}

pub fn sign_withdrawal_signature(
    keypair: &Keypair,
    signer_address: &Address,
    withdrawal_utxo: &OutPoint,
    claim_address: &Address,
    amount: Amount,
) -> Result<bitcoin::taproot::Signature, Box<dyn std::error::Error>> {
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

    let withdrawal_tx = Transaction {
        version: bitcoin::transaction::Version::non_standard(3),
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![txin],
        output: vec![txout],
    };

    let prevout = TxOut {
        value: Amount::from_sat(330),
        script_pubkey: signer_address.script_pubkey(),
    };

    let mut sighash_cache = bitcoin::sighash::SighashCache::new(withdrawal_tx.clone());

    let sighash = sighash_cache
        .taproot_key_spend_signature_hash(
            0,
            &bitcoin::sighash::Prevouts::One(0, &prevout),
            bitcoin::TapSighashType::SinglePlusAnyoneCanPay,
        )
        .unwrap();

    let sig = sign_with_tweak(*keypair, sighash, None);

    let taproot_signature = bitcoin::taproot::Signature {
        signature: sig,
        sighash_type: bitcoin::TapSighashType::SinglePlusAnyoneCanPay,
    };

    Ok(taproot_signature)
}

pub fn verify_withdrawal_signature(
    sig: &bitcoin::taproot::Signature,
    signer_address: &Address,
    withdrawal_utxo: &OutPoint,
    claim_address: &Address,
    amount: Amount,
) -> Result<(), Box<dyn std::error::Error>> {
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

    let withdrawal_tx = Transaction {
        version: bitcoin::transaction::Version::non_standard(3),
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![txin],
        output: vec![txout],
    };

    let prevout = TxOut {
        value: Amount::from_sat(330),
        script_pubkey: signer_address.script_pubkey(),
    };

    let mut sighash_cache = bitcoin::sighash::SighashCache::new(withdrawal_tx.clone());

    let sighash = sighash_cache
        .taproot_key_spend_signature_hash(
            0,
            &bitcoin::sighash::Prevouts::One(0, &prevout),
            bitcoin::TapSighashType::SinglePlusAnyoneCanPay,
        )
        .unwrap();

    SECP.verify_schnorr(
        &sig.signature,
        &bitcoin::secp256k1::Message::from_digest(*sighash.as_byte_array()),
        &XOnlyPublicKey::from_slice(&signer_address.script_pubkey().to_bytes()[2..34])?,
    )
    .map_err(|_| -> Box<dyn std::error::Error> { "Signature verification failed".into() })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::address::AddressType;
    use bitcoin::secp256k1::SecretKey;

    #[test]
    fn test_calculate_taproot_address() {
        let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
        let keypair = Keypair::from_secret_key(&SECP, &secret_key);
        let address = calculate_taproot_address(&keypair, Network::Testnet);
        assert_eq!(address.address_type(), Some(AddressType::P2tr));
    }

    #[test]
    fn test_generate_key_and_taproot_address() {
        let (keypair, address) = generate_key_and_taproot_address(Network::Testnet).unwrap();
        assert_eq!(address.address_type(), Some(AddressType::P2tr));
        // Verify that the address matches the keypair
        assert_eq!(
            calculate_taproot_address(&keypair, Network::Testnet),
            address
        );
    }

    #[test]
    fn test_confirm_private_key_storage_auto_yes() {
        assert!(confirm_private_key_storage(true).unwrap());
    }
}
