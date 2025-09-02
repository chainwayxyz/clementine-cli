// Deposit-related commands and logic for Clementine CLI

use crate::backend::create_deposit_account;
use crate::bitcoin_utils::calculate_deposit_address;
use crate::bitcoin_utils::sign_recovery_tx as utils_sign_recovery_tx;
use crate::config::BridgeCliConfig;
use crate::errors::BridgeCliError;
use crate::parameters::get_citrea_deposit_params;
use crate::structs::SecureKeypair;
use crate::structs::TaprootAddressWithPrefix;
use crate::wallet::Purpose;
use crate::withdrawal::{get_tx_details, get_txout_details};
use crate::{BitcoinAddress, CitreaAddress};
use bitcoin::key::Keypair;
use bitcoin::{Amount, FeeRate, OutPoint, Transaction, Txid};
use bitcoincore_rpc::RpcApi;
use eyre::Context;
use eyre::Result;
use url::Url;

pub(crate) enum DepositStatusEnum {
    New,
    InProgress,
    Completed,
    Unknown,
}

impl DepositStatusEnum {
    pub(crate) fn from_backend_status(status: &str) -> Self {
        match status {
            "new" => DepositStatusEnum::New,
            "minted" => DepositStatusEnum::Completed,
            "flushing_initiating" | "flushing_initiated" | "flushing_broadcasting" | "sent" => {
                DepositStatusEnum::InProgress
            }
            _ => DepositStatusEnum::Unknown,
        }
    }
    pub fn as_string(&self) -> String {
        match self {
            DepositStatusEnum::New => "New".to_string(),
            DepositStatusEnum::InProgress => "In Progress".to_string(),
            DepositStatusEnum::Completed => "Completed".to_string(),
            DepositStatusEnum::Unknown => "Unknown".to_string(),
        }
    }
}

/// Get deposit address from backend
pub async fn get_deposit_address(
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    config: &BridgeCliConfig,
) -> Result<BitcoinAddress, BridgeCliError> {
    if recovery_taproot_address.purpose != Purpose::Deposit {
        return Err(BridgeCliError::PurposeMismatch {
            expected: Purpose::Deposit,
            found: recovery_taproot_address.purpose,
        });
    }

    let (calculated_deposit_address, _) =
        calculate_deposit_address(citrea_address, &recovery_taproot_address.address, config)?;

    // Because backend is not available for regtest, don't cross check.
    if config.network == bitcoin::Network::Regtest {
        tracing::debug!("Regtest network is being used, not checking address against backend...");
        return Ok(calculated_deposit_address);
    }

    // Call backend to create deposit account
    let deposit_address =
        create_deposit_account(citrea_address, &recovery_taproot_address.address, config).await?;
    tracing::info!("Deposit address fetched from backend: {}", deposit_address);

    if deposit_address != calculated_deposit_address {
        return Err(BridgeCliError::CalculatedRecoveryTaprootAddressMismatch(
            calculated_deposit_address,
            deposit_address,
        ));
    }

    Ok(calculated_deposit_address)
}

pub async fn get_deposit_params(
    move_to_vault_txid: &Txid,
    config: &BridgeCliConfig,
) -> Result<Vec<u8>, BridgeCliError> {
    // 2. Get the prepare tx details
    let (move_to_vault_tx, move_to_vault_block, move_to_vault_block_height) =
        get_tx_details(move_to_vault_txid, config).await?;

    let move_to_vault_txout = get_txout_details(
        config,
        &move_to_vault_tx.input[0].previous_output.txid,
        move_to_vault_tx.input[0].previous_output.vout,
    )
    .await?;

    let deposit_params = get_citrea_deposit_params(
        move_to_vault_txout,
        &move_to_vault_tx,
        &move_to_vault_block,
        move_to_vault_block_height,
    )?;

    Ok(deposit_params)
}

/// Creates a signed raw transaction that can collect unminted funds from the
/// deposit transaction after 200 blocks.
#[allow(clippy::too_many_arguments)]
pub fn create_signed_recovery_tx(
    citrea_addr: &CitreaAddress,
    recovery_taproot_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    outpoint: &OutPoint,
    claim_addr: &BitcoinAddress,
    keypair: Keypair,
    fee_rate: u64,
    amount: f64,
    config: &BridgeCliConfig,
) -> Result<Transaction, BridgeCliError> {
    if recovery_taproot_address.purpose != Purpose::Deposit {
        return Err(BridgeCliError::PurposeMismatch {
            expected: Purpose::Deposit,
            found: recovery_taproot_address.purpose,
        });
    }

    // Convert BTC amount to satoshis if provided
    let deposit_amount = Amount::from_btc(amount)?;

    let keypair = SecureKeypair::new(keypair);

    let fee_rate = FeeRate::from_sat_per_vb_unchecked(fee_rate);

    let signed_tx = utils_sign_recovery_tx(
        &keypair,
        citrea_addr,
        &recovery_taproot_address.address,
        outpoint,
        deposit_amount,
        claim_addr,
        fee_rate,
        config,
    )?;

    Ok(signed_tx)
}

#[allow(clippy::too_many_arguments)]
pub fn verify_recovery_tx(
    recovery_tx: &Transaction,
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    amount: Option<f64>,
    config: &BridgeCliConfig,
) -> Result<(Txid, BitcoinAddress, Amount), BridgeCliError> {
    if recovery_taproot_address.purpose != Purpose::Deposit {
        return Err(BridgeCliError::PurposeMismatch {
            expected: Purpose::Deposit,
            found: recovery_taproot_address.purpose,
        });
    }

    let (txid, address, amount) = crate::bitcoin_utils::verify_recovery_tx(
        recovery_tx,
        citrea_address,
        &recovery_taproot_address.address,
        amount.map(|amount| Amount::from_btc(amount).unwrap()),
        config,
    )?;

    Ok((txid, address, amount))
}

/// Triest to broadcast recovery transaction using Mempool API. If that fails,
/// fallbacks to Bitcoin RPC. This is a basic wrapper and won't check if a tx
/// is valid or encoded correctly.
pub async fn broadcast_recovery_tx(
    config: &BridgeCliConfig,
    raw_tx: String,
) -> Result<Txid, BridgeCliError> {
    let mempool_api_txid =
        broadcast_recovery_tx_with_mempool(config.mempool_api_url.clone(), raw_tx.clone()).await;

    let rpc = config.connect_to_bitcoin_rpc().await?;
    let txid = rpc.send_raw_transaction(raw_tx).await.map_err(|btc_err| {
        BridgeCliError::CantBroadcastTransaction {
            mempool_api_error: mempool_api_txid.err().unwrap().to_string(),
            bitcoin_rpc_error: btc_err.to_string(),
        }
    })?;

    Ok(txid)
}

async fn broadcast_recovery_tx_with_mempool(
    mempool_url: Url,
    raw_tx: String,
) -> Result<Txid, BridgeCliError> {
    let client = reqwest::Client::new();

    let url = mempool_url
        .join(&format!("tx"))
        .wrap_err("Can't join url in get_tx_details_from_mempool")?;

    let response = client.post(url.as_str()).body(raw_tx).send().await?;

    if response.status().is_success() {
        let response_body = response.text().await?;
        tracing::debug!(
            "Response: {}",
            serde_json::to_string_pretty(&response_body)?
        );

        // Convert endiannes.
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

        Err(eyre::eyre!("Can't send raw: {} {}", status, error_text).into())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::{BitcoinConfig, BridgeCliConfig},
        deposit::broadcast_recovery_tx,
    };
    use bitcoin::{
        Address, Amount, OutPoint, Transaction, TxIn, TxOut, Txid, transaction::Version,
    };
    use bitcoincore_rpc::{Client, RpcApi};
    use secrecy::SecretString;
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
        let raw_tx = hex::encode(bitcoin::consensus::serialize(
            &signed_tx.transaction().unwrap(),
        ));

        raw_tx
    }

    #[tokio::test]
    #[ignore = "No utils present to make this a regular test, run manually"]
    async fn send_raw_tx_btc_cli() {
        let mut config = BridgeCliConfig::from_network(bitcoin::Network::Regtest);
        // Change this to your own env.
        config.bitcoin_config = Some(BitcoinConfig {
            url: Url::parse("http://localhost:18982/").unwrap(),
            password: SecretString::from("admin".to_string()),
            user: SecretString::from("admin".to_string()),
        });
        config.mempool_api_url = Url::from_str("http://127.0.0.1").unwrap();

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
        assert_eq!(
            txid,
            bitcoin::consensus::deserialize::<Transaction>(&hex::decode(raw_tx).unwrap())
                .unwrap()
                .compute_txid()
        );
    }

    #[tokio::test]
    #[ignore = "No utils present to make this a regular test, run manually"]
    async fn broadcast_recovery_tx_with_mempool() {
        color_eyre::install().expect("Failed to install color-eyre");

        let mut config = BridgeCliConfig::from_network(bitcoin::Network::Testnet4);
        config.bitcoin_config = Some(BitcoinConfig {
            url: Url::parse("http://localhost:22443/").unwrap(),
            password: SecretString::from("admin".to_string()),
            user: SecretString::from("admin".to_string()),
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
            super::broadcast_recovery_tx_with_mempool(config.mempool_api_url, raw_tx.clone())
                .await
                .unwrap();

        assert_eq!(
            txid,
            bitcoin::consensus::deserialize::<Transaction>(&hex::decode(raw_tx).unwrap())
                .unwrap()
                .compute_txid()
        );
    }
}
