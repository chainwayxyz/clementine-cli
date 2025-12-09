use crate::parameters::{CitreaMerkleProof, CitreaTransaction};
use crate::structs::Params;
use crate::types::BRIDGE_CONTRACT::BRIDGE_CONTRACTInstance;
use alloy::network::EthereumWallet;
use alloy::providers::RootProvider;
use alloy::providers::fillers::{
    BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller, WalletFiller,
};
use alloy::sol;
use alloy::sol_types::SolCall;
use alloy::sol_types::private;

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
    struct UTXO {
        bytes32 txId;
        bytes4 outputId;
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

        UTXO[] public withdrawalUTXOs;
        function getWithdrawalCount() external view returns (uint256);
    }
}

pub(crate) fn encode_citrea_deposit_params(
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

pub(crate) fn prepare_safe_withdraw_params(
    prepare_tx: &CitreaTransaction,
    prepare_proof: &CitreaMerkleProof,
    payout_tx: &CitreaTransaction,
    block_header: &[u8],
    destination_address_pubkey: &[u8],
) -> Params {
    Params {
        transaction: prepare_tx.into(),
        merkle_proof: prepare_proof.into(),
        payout_transaction: payout_tx.into(),
        block_header: private::Bytes::from(block_header.to_vec()),
        output_script_pk: private::Bytes::from(destination_address_pubkey.to_vec()),
    }
}

pub(crate) fn encode_safe_withdraw_params(
    prepare_tx: &Transaction,
    prepare_proof: &MerkleProof,
    payout_tx: &Transaction,
    block_header: &private::Bytes,
    output_script_pk: &private::Bytes,
) -> Vec<u8> {
    let call = BRIDGE_CONTRACT::safeWithdrawCall {
        prepareTx: prepare_tx.clone(),
        prepareProof: prepare_proof.clone(),
        payoutTx: payout_tx.clone(),
        blockHeader: block_header.clone(),
        withdrawalAddressPubKey: output_script_pk.clone(),
    };

    let data = call.abi_encode();
    data.to_vec()
}

pub type CitreaContract = BRIDGE_CONTRACTInstance<
    FillProvider<
        JoinFill<
            JoinFill<
                alloy::providers::Identity,
                JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>,
            >,
            WalletFiller<EthereumWallet>,
        >,
        RootProvider,
    >,
>;
