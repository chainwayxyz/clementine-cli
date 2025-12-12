// API utility functions for handling fallback patterns

use std::str::FromStr;

use crate::errors::BridgeCliError;
use crate::{BitcoinAddress, config::BridgeCliConfig, parse_transaction_hex};
use bitcoin::{Amount, Block, Transaction, TxOut, Txid};
use eyre::{Context, eyre};
use serde::Deserialize;
use serde_json::Value;
use url::Url;

#[derive(Debug)]
pub struct UtxoInfo {
    pub txid: bitcoin::Txid,
    pub vout: u32,
    pub value: Amount,
    pub block_height: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
pub struct UtxoStatus {
    pub confirmed: bool,
    pub block_height: Option<u64>,
    pub block_hash: Option<String>,
    pub block_time: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct EsploraUtxo {
    pub txid: String,
    pub vout: u32,
    pub status: UtxoStatus,
    pub value: u64,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct MempoolTx {
    pub txid: String,
    pub version: i32,
    pub locktime: u32,
    pub size: u32,
    pub weight: u32,
    pub fee: u64,
    pub status: Value,
    pub vin: Vec<Value>,
    pub vout: Vec<Value>,
}

async fn get_block_info_for_tx_from_esplora_api(
    txid: &Txid,
    config: &BridgeCliConfig,
) -> Result<(u64, String), BridgeCliError> {
    if config.esplora_rest_api.is_none() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Bitcoin Esplora API URL is not configured."
        )));
    }

    let url = config
        .esplora_rest_api
        .clone()
        .expect("Checked above")
        .join(&format!("tx/{txid}"))
        .wrap_err("Can't join url in get_tx_details_from_esplora_api")?;
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

    Ok((block_height, block_hash.to_string()))
}

pub async fn get_block_height_for_tx(
    txid: &Txid,
    config: &BridgeCliConfig,
) -> Result<u64, BridgeCliError> {
    match get_block_info_for_tx_from_esplora_api(txid, config).await {
        Ok((block_height, _)) => Ok(block_height),
        Err(esplora_error) => Err(esplora_error),
    }
}

/// Get transaction details using Bitcoin Esplora Api
pub async fn get_tx_details_from_esplora_api(
    txid: &Txid,
    config: &BridgeCliConfig,
) -> Result<(Transaction, Block, u32), BridgeCliError> {
    if config.esplora_rest_api.is_none() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Bitcoin Esplora Api URL is not configured."
        )));
    }

    let url = config
        .esplora_rest_api
        .clone()
        .expect("Checked above")
        .join(&format!("tx/{txid}/hex"))
        .wrap_err("Can't join url in get_tx_details_from_esplora_api")?;
    let response = reqwest::get(url)
        .await
        .map_err(|e| eyre!("Failed to fetch transaction hex for {txid}: {e}"))?;
    let tx_hex = response
        .text()
        .await
        .map_err(|e| eyre!("Failed to read transaction hex response for {txid}: {e}"))?;
    let tx: Transaction = parse_transaction_hex(&tx_hex)?;
    tracing::debug!("tx: {:?}", tx);

    let (block_height, block_hash) = get_block_info_for_tx_from_esplora_api(txid, config).await?;
    tracing::debug!("block_hash: {:?}", block_hash);
    tracing::debug!("block_height: {:?}", block_height);

    let url = config
        .esplora_rest_api
        .clone()
        .expect("Checked above")
        .join(&format!("block/{block_hash}/raw"))
        .wrap_err("Can't join url in get_tx_details_from_esplora_api")?;
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

/// Get transaction details with automatic fallback
pub async fn get_tx_details(
    txid: &Txid,
    config: &BridgeCliConfig,
) -> Result<(Transaction, Block, u32), BridgeCliError> {
    match get_tx_details_from_esplora_api(txid, config).await {
        Ok(result) => Ok(result),
        Err(esplora_error) => Err(esplora_error),
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

/// Triest to broadcast recovery transaction using Bitcoin Esplora Api. If that fails,
/// fallbacks to Bitcoin RPC. This is a basic wrapper and won't check if a tx
/// is valid or encoded correctly.
pub async fn broadcast_recovery_tx(
    config: &BridgeCliConfig,
    raw_tx: String,
) -> Result<Txid, BridgeCliError> {
    let esplora_api_txid =
        broadcast_recovery_tx_with_esplora_api(config.esplora_rest_api.clone(), raw_tx.clone())
            .await;
    match esplora_api_txid {
        Ok(txid) => Ok(txid),
        Err(e) => Err(e),
    }
}

async fn broadcast_recovery_tx_with_esplora_api(
    esplora_api: Option<Url>,
    raw_tx: String,
) -> Result<Txid, BridgeCliError> {
    if esplora_api.is_none() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Bitcoin Esplora Api URL is not configured."
        )));
    }

    let client = reqwest::Client::new();

    let url = esplora_api
        .expect("Checked above")
        .join("tx")
        .wrap_err("Can't join url in get_tx_details_from_esplora_api")?;

    let response = client.post(url.as_str()).body(raw_tx).send().await?;

    if response.status().is_success() {
        let response_body = response.text().await?;
        tracing::debug!(
            "Response: {}",
            serde_json::to_string_pretty(&response_body)?
        );

        // Convert endianness.
        let response_body: String = response_body
            .as_bytes()
            .chunks(2)
            .rev()
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect();

        let txid: Txid =
            bitcoin::consensus::deserialize(&hex::decode(response_body).unwrap()).unwrap();

        Ok(txid)
    } else {
        let status = response.status();
        let error_text = response.text().await?;
        tracing::error!("Deposit address request failed: {}", status);
        tracing::error!("Error response: {}", error_text);

        Err(eyre::eyre!(
            "Can't send raw tx\nError Status: {}\nError Text: {}",
            status,
            error_text
        )
        .into())
    }
}

pub(crate) async fn get_utxos(
    address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<Vec<UtxoInfo>, BridgeCliError> {
    let utxo_infos: Result<Vec<UtxoInfo>, BridgeCliError> =
        match get_utxos_from_esplora_api(address, config).await {
            Ok(utxos) => {
                let mut utxo_infos = Vec::new();
                for utxo in utxos {
                    let txid = Txid::from_str(&utxo.txid).map_err(|e| {
                        BridgeCliError::Eyre(eyre::eyre!(
                            "Failed to parse txid {}: {}",
                            utxo.txid,
                            e
                        ))
                    })?;
                    utxo_infos.push(UtxoInfo {
                        txid,
                        vout: utxo.vout,
                        value: Amount::from_sat(utxo.value),
                        block_height: utxo.status.block_height,
                    });
                }

                Ok(utxo_infos)
            }
            Err(e) => Err(e),
        };
    utxo_infos
}

pub(crate) async fn get_utxos_from_esplora_api(
    taproot_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<Vec<EsploraUtxo>, BridgeCliError> {
    if config.esplora_rest_api.is_none() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Bitcoin Esplora Api URL is not configured."
        )));
    }

    let url = config
        .esplora_rest_api
        .clone()
        .expect("Checked above")
        .join(&format!("address/{taproot_address}/utxo"))
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Failed to join esplora_rest_api: {e}")))?;
    let resp = reqwest::get(url).await?.error_for_status()?;
    let utxos: Vec<EsploraUtxo> = resp.json().await?;
    Ok(utxos)
}

pub async fn get_current_block_height(config: &BridgeCliConfig) -> Result<u64, BridgeCliError> {
    match get_current_block_height_from_esplora_api(config).await {
        Ok(height) => Ok(height),
        Err(esplora_error) => Err(esplora_error),
    }
}

async fn get_current_block_height_from_esplora_api(
    config: &BridgeCliConfig,
) -> Result<u64, BridgeCliError> {
    if config.esplora_rest_api.is_none() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Bitcoin Esplora Api URL is not configured."
        )));
    }

    let url = config
        .esplora_rest_api
        .clone()
        .expect("Checked above")
        .join("blocks/tip/height")
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Failed to join esplora_rest_api: {e}")))?;
    let resp = reqwest::get(url).await?.error_for_status()?;
    let height: u64 = resp.json().await?;
    Ok(height)
}

pub async fn get_esplora_api_mempool_txs(
    address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<Vec<MempoolTx>, BridgeCliError> {
    if config.esplora_rest_api.is_none() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Bitcoin Esplora Api URL is not configured."
        )));
    }

    let url = config
        .esplora_rest_api
        .clone()
        .expect("Checked above")
        .join(&format!("address/{address}/txs/mempool"))
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Failed to join esplora_rest_api: {e}")))?;
    let resp = reqwest::get(url).await?.error_for_status()?;
    let txs: Vec<MempoolTx> = resp.json().await?;
    Ok(txs)
}

pub async fn is_tx_on_chain(txid: &Txid, config: &BridgeCliConfig) -> Result<bool, BridgeCliError> {
    match is_tx_on_chain_with_esplora_api(txid, config).await {
        Ok(is_confirmed) => Ok(is_confirmed),
        Err(esplora_error) => Err(esplora_error),
    }
}

async fn is_tx_on_chain_with_esplora_api(
    txid: &Txid,
    config: &BridgeCliConfig,
) -> Result<bool, BridgeCliError> {
    if config.esplora_rest_api.is_none() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Bitcoin Esplora Api URL is not configured."
        )));
    }

    let url = config
        .esplora_rest_api
        .clone()
        .expect("Checked above")
        .join(&format!("tx/{}/status", txid))
        .map_err(|e| BridgeCliError::Eyre(eyre::eyre!("Failed to join esplora_rest_api: {e}")))?;
    let resp = reqwest::get(url).await?.error_for_status()?;
    tracing::debug!("Is tx on chain from esplora api response: {:?}", resp);
    let status: UtxoStatus = resp.json().await?;
    tracing::debug!("Is tx on chain from esplora api status: {:?}", status);
    Ok(status.confirmed)
}

#[cfg(test)]
mod tests {
    use crate::{
        broadcast_recovery_tx,
        config::{BitcoinConfig, BridgeCliConfig},
        parse_transaction_hex,
    };
    use bitcoin::{
        Address, Amount, OutPoint, Transaction, TxIn, TxOut, Txid, transaction::Version,
    };
    use bitcoincore_rpc::{Client, RpcApi};
    use std::str::FromStr;
    use url::Url;

    async fn create_raw_tx(
        rpc: Client,
        input_txid: Txid,
        vout: u32,
        address: Address,
        amount: Amount,
    ) -> String {
        let txin = TxIn {
            previous_output: OutPoint {
                txid: input_txid,
                vout,
            },
            ..Default::default()
        };
        let txout = TxOut {
            value: amount,
            script_pubkey: address.script_pubkey(),
        };
        let tx = Transaction {
            version: Version::TWO,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![txin],
            output: vec![txout],
        };

        let funded_tx = rpc.fund_raw_transaction(&tx, None, None).await.unwrap();
        let signed_tx = rpc
            .sign_raw_transaction_with_wallet(&funded_tx.transaction().unwrap(), None, None)
            .await
            .unwrap();

        hex::encode(bitcoin::consensus::serialize(
            &signed_tx.transaction().unwrap(),
        ))
    }

    #[tokio::test]
    #[ignore = "No utils present to make this a regular test, run manually"]
    async fn send_raw_tx_btc_cli() {
        let mut config = BridgeCliConfig::from_network(bitcoin::Network::Regtest);
        // WARNING: Change this to your own env.
        config.bitcoin_config = Some(BitcoinConfig {
            url: Url::parse("http://localhost:18982/").unwrap(),
            user: "admin".to_string(),
            password: "admin".to_string(),
        });
        // Needs to be invalid.
        config.esplora_rest_api = Some(Url::from_str("http://127.0.0.1").unwrap());

        let rpc = config.connect_to_bitcoin_rpc().await.unwrap();
        let address = rpc
            .get_new_address(None, None)
            .await
            .unwrap()
            .assume_checked();
        rpc.generate_to_address(101, &address).await.unwrap();

        let input_txid = rpc
            .send_to_address(
                &address,
                Amount::from_int_btc(1),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let raw_tx =
            create_raw_tx(rpc, input_txid, 0, address, Amount::from_btc(0.9).unwrap()).await;

        let txid = broadcast_recovery_tx(&config, raw_tx.clone())
            .await
            .unwrap();
        assert_eq!(txid, parse_transaction_hex(&raw_tx).unwrap().compute_txid());
    }

    #[tokio::test]
    #[ignore = "No utils present to make this a regular test, run manually"]
    async fn broadcast_recovery_tx_with_esplora_api() {
        color_eyre::install().expect("Failed to install color-eyre");

        let mut config = BridgeCliConfig::from_network(bitcoin::Network::Testnet4);
        config.bitcoin_config = Some(BitcoinConfig {
            url: Url::parse("http://localhost:22443/").unwrap(),
            user: "admin".to_string(),
            password: "admin".to_string(),
        });

        let rpc = config.connect_to_bitcoin_rpc().await.unwrap();

        let address = rpc
            .get_new_address(None, None)
            .await
            .unwrap()
            .assume_checked();
        println!("Address: {address:?}");

        // WARNING: CHANGE TXID TO ONE OF YOUR OWN
        let input_txid = Txid::from_str("").unwrap();

        let raw_tx = create_raw_tx(
            rpc,
            input_txid,
            0,
            address,
            Amount::from_btc(0.009).unwrap(), // WARNING: UPDATE SENT AMOUNT DEPENDING ON THE INPUT UTXO
        )
        .await;

        let txid =
            super::broadcast_recovery_tx_with_esplora_api(config.esplora_rest_api, raw_tx.clone())
                .await
                .unwrap();

        assert_eq!(txid, parse_transaction_hex(&raw_tx).unwrap().compute_txid());
    }
}
