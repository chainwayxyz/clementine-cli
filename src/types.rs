use crate::parameters::{CitreaMerkleProof, CitreaTransaction};
use alloy_sol_types::private;
use alloy_sol_types::{SolCall, sol};

sol! {
    #[derive(Debug)]
    struct Transaction {
        bytes4 version;
        bytes2 flag;
        bytes vin;
        bytes vout;
        bytes witness;
        bytes4 locktime;
    }

    #[derive(Debug)]
    struct MerkleProof {
        bytes intermediateNodes;
        uint256 blockHeight;
        uint256 index;
    }

    #[derive(Debug)]
    #[sol(rpc)]
    interface BRIDGE_CONTRACT {
        function deposit(
            Transaction calldata moveTx,
            MerkleProof calldata proof,
            bytes32 shaScriptPubkeys
        ) external;

        function safeWithdraw(
            Transaction calldata prepareTx,
            MerkleProof calldata prepareProof,
            Transaction calldata payoutTx,
            bytes calldata blockHeader,
            bytes memory withdrawalAddressPubKey
        ) external payable;
    }
}

pub fn encode_citrea_deposit_params(
    move_tx: &CitreaTransaction,
    proof: &CitreaMerkleProof,
    sha_script_pubkeys: [u8; 32],
) -> Vec<u8> {
    // Create the deposit call
    let call = BRIDGE_CONTRACT::depositCall {
        moveTx: move_tx.into(),
        proof: proof.into(),
        shaScriptPubkeys: private::FixedBytes::from(sha_script_pubkeys),
    };

    // Return the encoded calldata (without the function selector)
    let data = call.abi_encode();
    tracing::debug!("data: {:?}", data);
    data[4..].to_vec()
}

impl From<&CitreaTransaction> for Transaction {
    fn from(tx: &CitreaTransaction) -> Self {
        Transaction {
            version: private::FixedBytes::from(tx.version),
            locktime: private::FixedBytes::from(tx.locktime),
            flag: private::FixedBytes::from(tx.flag),
            vin: private::Bytes::from(tx.vin.clone()),
            vout: private::Bytes::from(tx.vout.clone()),
            witness: private::Bytes::from(tx.witness.clone()),
        }
    }
}

impl From<&CitreaMerkleProof> for MerkleProof {
    fn from(proof: &CitreaMerkleProof) -> Self {
        MerkleProof {
            intermediateNodes: private::Bytes::from(proof.intermediate_nodes.clone()),
            blockHeight: private::U256::from(proof.block_height),
            index: private::U256::from(proof.index),
        }
    }
}

pub fn prepare_safe_withdraw_params(
    prepare_tx: &CitreaTransaction,
    prepare_proof: &CitreaMerkleProof,
    payout_tx: &CitreaTransaction,
    block_header: &[u8],
    withdrawal_address_pubkey: &[u8],
) -> (
    Transaction,
    MerkleProof,
    Transaction,
    private::Bytes,
    private::Bytes,
) {
    (
        prepare_tx.into(),
        prepare_proof.into(),
        payout_tx.into(),
        private::Bytes::from(block_header.to_vec()),
        private::Bytes::from(withdrawal_address_pubkey.to_vec()),
    )
}
