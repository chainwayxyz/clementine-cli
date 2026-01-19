//! # Parameter Builder For Citrea Requests

use crate::bitcoin_merkle::BitcoinMerkleTree;
use crate::errors::BridgeCliError;
use crate::types::encode_citrea_deposit_params;

use eyre::Result;

use bitcoin::OutPoint;
use bitcoin::ScriptBuf;
use bitcoin::Sequence;
use bitcoin::TxIn;
use bitcoin::TxOut;
use bitcoin::Witness;
use bitcoin::consensus::Encodable;
use bitcoin::hashes::Hash;
use bitcoin::hashes::sha256;
use bitcoin::{Block, Transaction, Txid};
use eyre::Context;

/// Returns merkle proof for a given transaction (via txid) in a block.
fn get_block_merkle_proof(
    block: &Block,
    target_txid: Txid,
    is_witness_merkle_proof: bool,
) -> Result<(usize, Vec<u8>), BridgeCliError> {
    let mut txid_index = None;
    let txids = block
        .txdata
        .iter()
        .enumerate()
        .map(|(i, tx)| {
            let txid = tx.compute_txid();
            if txid == target_txid {
                txid_index = Some(i);
            }

            if is_witness_merkle_proof {
                if i == 0 {
                    [0; 32]
                } else {
                    let wtxid = tx.compute_wtxid();
                    wtxid.as_byte_array().to_owned()
                }
            } else {
                txid.as_byte_array().to_owned()
            }
        })
        .collect::<Vec<_>>();

    let txid_index = txid_index.ok_or_else(|| {
        BridgeCliError::Eyre(eyre::eyre!("Transaction {target_txid} not found in block"))
    })?;

    let merkle_tree = BitcoinMerkleTree::new(txids.clone())?;
    let witness_idx_path = merkle_tree.get_idx_path(txid_index.try_into().unwrap());

    let _root = merkle_tree.calculate_root_with_merkle_proof(
        txids[txid_index],
        txid_index.try_into().unwrap(),
        witness_idx_path.clone(),
    );

    Ok((txid_index, witness_idx_path.into_iter().flatten().collect()))
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct CitreaTransaction {
    pub version: [u8; 4],
    pub flag: [u8; 2],
    pub vin: Vec<u8>,
    pub vout: Vec<u8>,
    pub witness: Vec<u8>,
    pub locktime: [u8; 4],
}

// implement Debug for CitreaTransaction
impl std::fmt::Debug for CitreaTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "tx:\nversion: 0x{}\nflag: 0x{}\nvin: 0x{}\nvout: 0x{}\nwitness: 0x{}\nlocktime: 0x{}",
            hex::encode(self.version),
            hex::encode(self.flag),
            hex::encode(&self.vin),
            hex::encode(&self.vout),
            hex::encode(&self.witness),
            hex::encode(self.locktime)
        )
    }
}

fn get_transaction_details_for_citrea(
    transaction: &Transaction,
) -> Result<CitreaTransaction, BridgeCliError> {
    let version = (transaction.version.0 as u32).to_le_bytes();
    let flag: u16 = 1;

    let vin = [
        vec![transaction.input.len() as u8],
        transaction
            .input
            .iter()
            .map(|x| bitcoin::consensus::serialize(&x))
            .collect::<Vec<_>>()
            .into_iter()
            .flatten()
            .collect::<Vec<u8>>(),
    ]
    .concat();

    let vout = [
        vec![transaction.output.len() as u8],
        transaction
            .output
            .iter()
            .map(|x| bitcoin::consensus::serialize(&x))
            .collect::<Vec<_>>()
            .into_iter()
            .flatten()
            .collect::<Vec<u8>>(),
    ]
    .concat();

    let witness: Vec<u8> = transaction
        .input
        .iter()
        .map(|param| {
            let mut raw = Vec::new();
            param
                .witness
                .consensus_encode(&mut raw)
                .wrap_err("Can't encode param")?;

            Ok::<Vec<u8>, eyre::Error>(raw)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<u8>>();

    let locktime = bitcoin::consensus::serialize(&transaction.lock_time);
    let locktime: [u8; 4] = locktime.try_into().unwrap();
    Ok(CitreaTransaction {
        version,
        flag: flag.to_be_bytes(),
        vin,
        vout,
        witness,
        locktime,
    })
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct CitreaMerkleProof {
    pub intermediate_nodes: Vec<u8>,
    pub block_height: u32,
    pub index: usize,
}

// implement Debug for CitreaMerkleProof

impl std::fmt::Debug for CitreaMerkleProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MerkleProof:\nintermediate_nodes: 0x{}\nblock_height: {}\nindex: {}",
            hex::encode(&self.intermediate_nodes),
            self.block_height,
            self.index
        )
    }
}

/// Structured return type for safe withdraw parameters instead of complex tuple
#[derive(Debug)]
pub struct SafeWithdrawParams {
    pub prepare_tx: CitreaTransaction,
    pub prepare_proof: CitreaMerkleProof,
    pub payout_tx: CitreaTransaction,
    pub block_header: Vec<u8>,
    pub output_script_pk: Vec<u8>,
}

fn get_transaction_merkle_proof_for_citrea(
    block_height: u32,
    block: &Block,
    txid: Txid,
    is_witness_merkle_proof: bool,
) -> Result<CitreaMerkleProof, BridgeCliError> {
    let (index, merkle_proof) = get_block_merkle_proof(block, txid, is_witness_merkle_proof)?;

    Ok(CitreaMerkleProof {
        intermediate_nodes: merkle_proof,
        block_height,
        index,
    })
}

pub(crate) fn get_citrea_deposit_params(
    prevout: TxOut,
    move_to_vault_tx: &Transaction,
    move_to_vault_block: &Block,
    move_to_vault_block_height: u32,
) -> Result<Vec<u8>, BridgeCliError> {
    let move_to_vault_tx_struct = get_transaction_details_for_citrea(move_to_vault_tx)?;

    let move_to_vault_tx_mp = get_transaction_merkle_proof_for_citrea(
        move_to_vault_block_height,
        move_to_vault_block,
        move_to_vault_tx.compute_txid(),
        true,
    )?;

    let mut enc_script_pubkeys = sha256::Hash::engine();

    prevout
        .script_pubkey
        .consensus_encode(&mut enc_script_pubkeys)
        .unwrap();
    let sha_script_pubkeys = sha256::Hash::from_engine(enc_script_pubkeys);

    let sha_script_pks: [u8; 32] = sha_script_pubkeys
        .as_byte_array()
        .to_vec()
        .try_into()
        .unwrap();

    let data = encode_citrea_deposit_params(
        &move_to_vault_tx_struct,
        &move_to_vault_tx_mp,
        sha_script_pks,
    );
    Ok(data)
}

pub(crate) fn get_citrea_safe_withdraw_params(
    withdrawal_utxo: &OutPoint,
    payout_output: &bitcoin::TxOut,
    sig: &bitcoin::taproot::Signature,
    prepare_tx: &Transaction,
    prepare_tx_block: &Block,
    prepare_tx_block_height: u32,
) -> Result<SafeWithdrawParams, BridgeCliError> {
    let prepare_tx_struct = get_transaction_details_for_citrea(prepare_tx)?;

    let prepare_tx_mp = get_transaction_merkle_proof_for_citrea(
        prepare_tx_block_height,
        prepare_tx_block,
        withdrawal_utxo.txid,
        false,
    )?;

    let txin = TxIn {
        previous_output: *withdrawal_utxo,
        script_sig: ScriptBuf::default(),
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        witness: Witness::default(),
    };

    let mut payout_tx = Transaction {
        version: bitcoin::transaction::Version::non_standard(3),
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![txin],
        output: vec![payout_output.clone()],
    };

    let mut witness = bitcoin::Witness::new();
    witness.push(sig.serialize());

    payout_tx.input[0].witness = witness;

    let payout_tx_params = get_transaction_details_for_citrea(&payout_tx)?;

    let prepare_tx_block_header = prepare_tx_block.header;

    let block_header_bytes = bitcoin::consensus::serialize(&prepare_tx_block_header);

    let output_script_pk_bytes = &bitcoin::consensus::serialize(&payout_tx.output[0].script_pubkey)
        .iter()
        .skip(1)
        .copied()
        .collect::<Vec<u8>>();

    tracing::debug!(
        "{:#?}",
        (
            prepare_tx_struct.clone(),
            prepare_tx_mp.clone(),
            payout_tx_params.clone(),
            hex::encode(block_header_bytes.clone()),
            hex::encode(output_script_pk_bytes.clone()),
        )
    );

    Ok(SafeWithdrawParams {
        prepare_tx: prepare_tx_struct,
        prepare_proof: prepare_tx_mp,
        payout_tx: payout_tx_params,
        block_header: block_header_bytes,
        output_script_pk: output_script_pk_bytes.to_vec(),
    })
}
