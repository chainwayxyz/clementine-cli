use crate::debug;
use crate::parameters::{CitreaMerkleProof, CitreaTransaction};
use alloy_sol_types::SolCall;
use alloy_sol_types::private;

alloy_sol_types::sol! {
    struct Transaction {
        bytes4 version;
        bytes2 flag;
        bytes vin;
        bytes vout;
        bytes witness;
        bytes4 locktime;
    }

    struct MerkleProof {
        bytes intermediateNodes;
        uint256 blockHeight;
        uint256 index;
    }

    function deposit(
        Transaction calldata moveTx,
        MerkleProof calldata proof,
        bytes32 shaScriptPubkeys
    ) external;
}

pub fn encode_citrea_deposit_params(
    move_tx: &CitreaTransaction,
    proof: &CitreaMerkleProof,
    sha_script_pubkeys: [u8; 32],
) -> Vec<u8> {
    let transaction = Transaction {
        version: private::FixedBytes::from(move_tx.version),
        locktime: private::FixedBytes::from(move_tx.locktime),
        flag: private::FixedBytes::from(move_tx.flag),
        vin: private::Bytes::from(move_tx.vin.clone()),
        vout: private::Bytes::from(move_tx.vout.clone()),
        witness: private::Bytes::from(move_tx.witness.clone()),
    };

    let merkle_proof = MerkleProof {
        intermediateNodes: private::Bytes::from(proof.intermediate_nodes.clone()),
        blockHeight: private::U256::from(proof.block_height),
        index: private::U256::from(proof.index),
    };
    // Create the deposit call
    let call = depositCall {
        moveTx: transaction,
        proof: merkle_proof,
        shaScriptPubkeys: private::FixedBytes::from(sha_script_pubkeys),
    };

    // Return the encoded calldata (without the function selector)
    let data = call.abi_encode();
    debug!("data: {:?}", data);
    data[4..].to_vec()
}
