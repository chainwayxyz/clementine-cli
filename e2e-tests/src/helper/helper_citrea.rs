use crate::bitcoin::BitcoinRpcExt;
use anyhow::{Context, Result, anyhow};
use bitcoin::Txid;
use bitcoincore_rpc::Client;
use citrea_e2e::{
    bitcoin::DEFAULT_FINALITY_DEPTH, config::SequencerConfig, node::Node, traits::NodeT,
};
use clementine_cli::config::BridgeCliConfig;
use clementine_cli::deposit::get_deposit_params;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;

use crate::citrea_bridge;

pub async fn wait_for_citrea(sequencer: &Node<SequencerConfig>) -> Result<()> {
    let max_retries = 30;

    for _ in 0..max_retries {
        match get_block_number(sequencer).await {
            Ok(block_number) => {
                tracing::info!("Citrea is up at block number {}", block_number);
                return Ok(());
            }
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }

    Err(anyhow!("Citrea cannot be reached after multiple attempts"))
}

/// Get the current block number from Citrea
pub async fn get_block_number(sequencer: &Node<SequencerConfig>) -> Result<u64> {
    let client = sequencer.client.http_client();
    let block_number_hex: String = client
        .request("eth_blockNumber", rpc_params![])
        .await
        .map_err(|e| anyhow!("Failed to get block number: {}", e))?;

    let block_number = u64::from_str_radix(block_number_hex.trim_start_matches("0x"), 16)?;
    Ok(block_number)
}

pub async fn ensure_bridge_contract_deployed(sequencer: &Node<SequencerConfig>) -> Result<()> {
    sequencer
        .client
        .send_publish_batch_request()
        .await
        .map_err(|e| anyhow!("Failed to publish initial batch: {}", e))?;
    sequencer
        .wait_for_l2_height(1, None)
        .await
        .map_err(|e| anyhow!("Failed waiting for L2 height: {}", e))?;

    let bridge_contract_client =
        citrea_bridge::CitreaBridgeClient::from_sequencer_config(sequencer, None)?;

    let deposit_amount = bridge_contract_client.get_deposit_amount().await?;

    if deposit_amount == 0 {
        Err(anyhow!(
            "Bridge contract is not deployed or not initialized"
        ))?;
    }

    tracing::info!(
        "Bridge contract is deployed with deposit amount: {}",
        deposit_amount
    );
    Ok(())
}

/// Get the balance of an address on Citrea
pub async fn get_citrea_balance(sequencer: &Node<SequencerConfig>, address: &str) -> Result<u64> {
    let client = sequencer.client.http_client();
    let balance_hex: String = client
        .request("eth_getBalance", rpc_params![address, "latest"])
        .await
        .map_err(|e| anyhow!("Failed to get balance: {}", e))?;

    let balance = u64::from_str_radix(balance_hex.trim_start_matches("0x"), 16)?;
    Ok(balance)
}

/// Wait for a balance change on Citrea and return the updated balance.
pub async fn wait_for_balance_change(
    sequencer: &Node<SequencerConfig>,
    address: &str,
    initial_balance: u64,
) -> Result<u64> {
    let max_retries = 60;

    for _ in 0..max_retries {
        let current = get_citrea_balance(sequencer, address).await?;
        if current != initial_balance {
            return Ok(current);
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    Err(anyhow!(
        "Citrea balance did not change after multiple attempts"
    ))
}

pub async fn deposit_to_citrea(
    rpc: &Client,
    sequencer: &Node<SequencerConfig>,
    move_txid: Txid,
    config: &BridgeCliConfig,
) -> Result<()> {
    let seq_client = sequencer.client().http_client().clone();
    // Commit BTC and L2 blocks for the BTC Light Client to include the deposit.
    rpc.generate_blocks(DEFAULT_FINALITY_DEPTH + 2, None)
        .await
        .unwrap();

    force_sequencer_to_commit(sequencer).await.unwrap();

    rpc.generate_blocks(DEFAULT_FINALITY_DEPTH + 2, None)
        .await
        .unwrap();

    // Force sequencer to produce a block to catch up with missed DA blocks
    tracing::info!("Forcing sequencer to sync with Bitcoin blocks...");
    sequencer.client.send_publish_batch_request().await?;

    tracing::info!("Committing deposit to Citrea...");

    let params = get_deposit_params(&move_txid, config).await?;

    let _response: () = seq_client
        .request(
            "citrea_sendRawDepositTransaction",
            rpc_params!(hex::encode(params)),
        )
        .await
        .context("Failed to send deposit transaction")?;

    force_sequencer_to_commit(sequencer).await.unwrap();

    tracing::info!("Deposit operations are successful.");
    Ok(())
}

pub async fn force_sequencer_to_commit(sequencer: &Node<SequencerConfig>) -> Result<()> {
    for _ in 0..sequencer.max_l2_blocks_per_commitment() {
        sequencer
            .client
            .send_publish_batch_request()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to publish block: {:?}", e))?;
    }
    Ok(())
}
