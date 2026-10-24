use crate::bitcoin::BitcoinRpcExt as _;
use anyhow::{Context, Result, anyhow};
use bitcoin::{Address, Amount, Txid};
use bitcoincore_rpc::RpcApi;
use citrea_e2e::bitcoin::{BitcoinNode, DEFAULT_FINALITY_DEPTH};
use citrea_e2e::clementine::client::ClementineAggregatorTestClient;
use citrea_e2e::traits::NodeT;

/// Result of submitting a deposit and move transaction
#[derive(Debug, Clone, Copy)]
pub struct SubmittedDeposit {
    pub deposit_txid: Txid,
    pub move_txid: Txid,
}

/// Parse a hex EVM address string (with or without 0x) into a 20-byte array
pub fn parse_evm_address_to_20(evm_hex: &str) -> anyhow::Result<[u8; 20]> {
    let hex_str = evm_hex.trim_start_matches("0x");
    let bytes = hex::decode(hex_str).map_err(|e| anyhow!("Invalid EVM address: {}", e))?;
    anyhow::ensure!(
        bytes.len() == 20,
        "Invalid EVM address length: {}",
        bytes.len()
    );
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

pub async fn submit_deposit_and_move(
    bitcoin_node: &BitcoinNode,
    aggregator_client: &mut ClementineAggregatorTestClient,
    deposit_address: &Address,
    deposit_amount: Amount,
    evm_address: [u8; 20],
    recovery_taproot_address: String,
) -> Result<SubmittedDeposit> {
    // Create deposit transaction on Bitcoin
    let deposit_txid = bitcoin_node
        .send_to_address(
            deposit_address,
            deposit_amount,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await?;

    bitcoin_node.generate(DEFAULT_FINALITY_DEPTH).await?;

    let deposit_tx = bitcoin_node.get_transaction(&deposit_txid, None).await?;
    anyhow::ensure!(
        deposit_tx.info.blockhash.is_some(),
        "Deposit tx not yet confirmed"
    );

    // Prepare deposit data for Clementine
    let tx_bytes: bitcoin::Transaction = bitcoin::consensus::encode::deserialize(&deposit_tx.hex)?;

    // Find the output index that goes to the bridge address
    let mut deposit_vout = None;
    for (index, output) in tx_bytes.output.iter().enumerate() {
        if let Ok(address) = Address::from_script(&output.script_pubkey, bitcoin::Network::Regtest)
            && &address == deposit_address
        {
            deposit_vout = Some(index as u32);
            break;
        }
    }

    let vout = deposit_vout.ok_or_else(|| anyhow::anyhow!("No output found to bridge address"))?;
    let deposit_request = citrea_e2e::clementine::client::clementine::Deposit {
        deposit_outpoint: Some(citrea_e2e::clementine::client::clementine::Outpoint {
            txid: Some(citrea_e2e::clementine::client::clementine::Txid {
                txid: {
                    let mut bytes = hex::decode(deposit_txid.to_string())
                        .map_err(|e| anyhow::anyhow!("Invalid txid hex: {}", e))?;
                    bytes.reverse(); // Bitcoin txids are in reverse byte order
                    bytes
                },
            }),
            vout,
        }),
        deposit_data: Some(
            citrea_e2e::clementine::client::clementine::deposit::DepositData::BaseDeposit(
                citrea_e2e::clementine::client::clementine::BaseDeposit {
                    evm_address: evm_address.to_vec(),
                    recovery_taproot_address,
                },
            ),
        ),
    };

    // Submit deposit to Clementine
    let move_tx_response = aggregator_client
        .new_deposit(deposit_request)
        .await
        .context("Failed to submit deposit to Clementine")?;

    let move_tx: bitcoin::Transaction =
        bitcoin::consensus::encode::deserialize(&move_tx_response.raw_tx)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize move transaction: {}", e))?;

    bitcoin_node
        .client()
        .send_cpfp_tx(&move_tx, None)
        .await
        .context("Failed to send move transaction")?;

    bitcoin_node
        .client()
        .mine_once_after_in_mempool(move_tx.compute_txid(), Some("Move tx"), None)
        .await?;

    Ok(SubmittedDeposit {
        deposit_txid,
        move_txid: move_tx.compute_txid(),
    })
}
