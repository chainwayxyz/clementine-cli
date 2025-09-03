// API utility functions for handling fallback patterns

use crate::config::BridgeCliConfig;
use crate::errors::BridgeCliError;
use bitcoin::{Block, Transaction, TxOut, Txid};
use bitcoincore_rpc::{Client, RpcApi};
use eyre::{Context, eyre};
use serde_json::Value;

/// Get transaction details using mempool API
pub async fn get_tx_details_from_mempool(
    txid: &Txid,
    config: &BridgeCliConfig,
) -> Result<(Transaction, Block, u32), BridgeCliError> {
    let url = config
        .mempool_api_url
        .join(&format!("tx/{txid}/hex"))
        .wrap_err("Can't join url in get_tx_details_from_mempool")?;
    let response = reqwest::get(url)
        .await
        .map_err(|e| eyre!("Failed to fetch transaction hex for {txid}: {e}"))?;
    let tx_hex = response
        .text()
        .await
        .map_err(|e| eyre!("Failed to read transaction hex response for {txid}: {e}"))?;
    let tx: Transaction = bitcoin::consensus::deserialize(&hex::decode(tx_hex)?)?;
    tracing::debug!("tx: {:?}", tx);

    let url = config
        .mempool_api_url
        .join(&format!("tx/{txid}"))
        .wrap_err("Can't join url in get_tx_details_from_mempool")?;
    let response = reqwest::get(url)
        .await
        .wrap_err("Failed to fetch transaction data: {}")?;
    let tx_data: Value = response
        .json()
        .await
        .wrap_err("Failed to parse transaction data: {}")?;
    tracing::debug!("tx_data: {:?}", tx_data);
    let block_hash = tx_data["status"]["block_hash"]
        .as_str()
        .ok_or(eyre!("Block hash not found"))?;
    let block_height = tx_data["status"]["block_height"]
        .as_u64()
        .ok_or(eyre!("Block height not found"))?;
    tracing::debug!("block_hash: {:?}", block_hash);
    tracing::debug!("block_height: {:?}", block_height);

    let url = config
        .mempool_api_url
        .join(&format!("block/{block_hash}/raw"))
        .wrap_err("Can't join url in get_tx_details_from_mempool")?;
    let response = reqwest::get(url)
        .await
        .map_err(|e| eyre!("Failed to fetch block raw data for {block_hash}: {e}"))?;
    let block_raw = response
        .bytes()
        .await
        .map_err(|e| eyre!("Failed to read block raw bytes for {block_hash}: {e}"))?;
    tracing::debug!("block_raw: {:?}", block_raw);
    let block: Block = bitcoin::consensus::deserialize(&block_raw)?;
    tracing::debug!("block: {:?}", block);
    Ok((tx, block, block_height as u32))
}

/// Get transaction details using Bitcoin RPC
pub async fn get_tx_details_from_rpc(
    rpc: &Client,
    txid: &Txid,
) -> Result<(Transaction, Block, u32), BridgeCliError> {
    let tx = rpc.get_raw_transaction(txid, None).await?;
    let tx_info = rpc.get_raw_transaction_info(txid, None).await?;
    if tx_info.blockhash.is_none() {
        return Err(eyre!("Block hash not found, maybe not confirmed yet").into());
    }
    let block = rpc.get_block(&tx_info.blockhash.unwrap()).await?;
    let block_height = rpc
        .get_block_header_info(&tx_info.blockhash.unwrap())
        .await?
        .height;
    tracing::debug!("tx_info: {:?}", tx_info);
    tracing::debug!("block: {:?}", block);
    tracing::debug!("block_height: {:?}", block_height);

    Ok((tx, block, block_height as u32))
}

/// Get transaction details with automatic fallback
pub async fn get_tx_details(
    txid: &Txid,
    config: &BridgeCliConfig,
) -> Result<(Transaction, Block, u32), BridgeCliError> {
    match get_tx_details_from_mempool(txid, config).await {
        Ok(result) => Ok(result),
        Err(mempool_error) => {
            tracing::warn!(
                "Mempool API failed for get_tx_details: {}, falling back to Bitcoin RPC",
                mempool_error
            );

            if config.bitcoin_config.is_some() {
                let rpc = config.connect_to_bitcoin_rpc().await?;
                get_tx_details_from_rpc(&rpc, txid).await
            } else {
                Err(mempool_error)
            }
        }
    }
}

/// Get transaction output details with automatic fallback
pub async fn get_txout_details(
    config: &BridgeCliConfig,
    txid: &Txid,
    vout: u32,
) -> Result<TxOut, BridgeCliError> {
    let (tx, _, _) = get_tx_details(txid, config).await?;
    let txout = tx
        .output
        .get(vout as usize)
        .ok_or::<BridgeCliError>(eyre!("Txout not found").into())?;

    Ok(txout.clone())
}
