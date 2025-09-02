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
use crate::wallet::passphrase::prompt_unlock_passphrase;
use crate::wallet::wallet_utils::load_key;
use crate::withdrawal::{get_tx_details, get_txout_details};
use crate::{BitcoinAddress, CitreaAddress};
use bitcoin::key::Keypair;
use bitcoin::{Amount, FeeRate, OutPoint, Transaction, Txid};
use bitcoincore_rpc::RpcApi;
use eyre::Context;
use eyre::Result;
use serde_json::json;
use url::Url;

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
    fee_rate: Option<u64>,
    amount: Option<f64>,
    config: &BridgeCliConfig,
) -> Result<Transaction, BridgeCliError> {
    if recovery_taproot_address.purpose != Purpose::Deposit {
        return Err(BridgeCliError::PurposeMismatch {
            expected: Purpose::Deposit,
            found: recovery_taproot_address.purpose,
        });
    }

    // Convert BTC amount to satoshis if provided
    let deposit_amount = match amount {
        Some(btc) => Some(Amount::from_btc(btc)?),
        None => None,
    };

    let keypair = SecureKeypair::new(keypair);

    let fee_rate_opt = fee_rate.map(FeeRate::from_sat_per_vb_unchecked);
    let signed_tx = utils_sign_recovery_tx(
        &keypair,
        citrea_addr,
        &recovery_taproot_address.address,
        outpoint,
        deposit_amount,
        claim_addr,
        fee_rate_opt,
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

    let response = client
        .post(url.as_str())
        .header("Content-Type", "application/text")
        .body(raw_tx)
        .send()
        .await?;
    println!("res {response:?}");

    if response.status().is_success() {
        let response_body: serde_json::Value = response.json().await?;
        println!(
            "Response: {}",
            serde_json::to_string_pretty(&response_body)?
        );
        // parse the json and get the taproot_addr and parse it to an address
        // let taproot_addr = response_body["taproot_addr"].as_str().unwrap();
        // let taproot_addr = parse_taproot_address(taproot_addr, config.network)?;

        // Ok(taproot_addr)
    } else {
        let status = response.status();
        let error_text = response.text().await?;
        println!("Deposit address request failed: {}", status);
        println!("Error response: {}", error_text);

        // Err(eyre::eyre!(
        //     "Backend request failed with status: {} {}",
        //     status,
        //     error_text
        // )
        // .into())
    }

    todo!()
}

#[cfg(test)]
mod tests {
    use crate::{
        config::{BitcoinConfig, BridgeCliConfig},
        deposit::broadcast_recovery_tx,
    };
    use bitcoin::{Amount, OutPoint, Transaction, TxIn, TxOut, transaction::Version};
    use bitcoincore_rpc::RpcApi;
    use secrecy::SecretString;
    use std::str::FromStr;
    use url::Url;

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

        let in_txid = rpc
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

        let txin = TxIn {
            previous_output: OutPoint {
                txid: in_txid,
                vout: 0,
            },
            ..Default::default()
        };
        let txout = TxOut {
            value: Amount::from_btc(0.9).unwrap(),
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

        let txid = broadcast_recovery_tx(&config, raw_tx).await.unwrap();
        assert_eq!(txid, signed_tx.transaction().unwrap().compute_txid());
    }
}

//     use crate::config::{BridgeCliConfig, UNSPENDABLE_XONLY_PUBKEY};
//     use crate::structs::TaprootAddressWithPrefix;
//     use crate::wallet::address::generate_address_from_mnemonic;
//     use crate::wallet::mnemonic::generate_mnemonic;
//     use crate::{CitreaAddress, create_signed_recovery_tx};
//     use bitcoin::Address;
//     use bitcoin::consensus::Encodable;
//     use bitcoin::hashes::Hash;
//     use bitcoin::key::Keypair;
//     use bitcoin::secp256k1::{Secp256k1, SecretKey};
//     use bitcoin::{
//         Amount, OutPoint, ScriptBuf, Transaction, TxIn, TxOut, Txid, transaction::Version,
//     };
//     use std::str::FromStr;

// #[tokio::test]
// async fn broadcast_recovery_tx_with_mempool() {
//     let config = BridgeCliConfig::from_network(bitcoin::Network::Testnet4);

//     let citrea_addr = CitreaAddress::from_slice(&[01u8; 20]);
//     let mnemonic = generate_mnemonic().unwrap();
//     let recovery_taproot_address = generate_address_from_mnemonic(
//         &mnemonic,
//         bitcoin::Network::Testnet4,
//         crate::wallet::Purpose::Deposit,
//     )
//     .unwrap();
//     let outpoint = OutPoint {
//         txid: Txid::from_str(
//             "32f8a6943317aa22c06d8d5a4870858494810750e33c4968ba78a76abb157363",
//         )
//         .unwrap(),
//         vout: 0,
//     };

//     let keypair = Keypair::from_secret_key(
//         &Secp256k1::new(),
//         &SecretKey::from_slice(&[1u8; 32]).unwrap(),
//     );

//     let x = create_signed_recovery_tx(
//         &citrea_addr,
//         &recovery_taproot_address,
//         &outpoint,
//         &recovery_taproot_address.address,
//         keypair,
//         None,
//         None,
//         &config,
//     )
//     .unwrap();
//     let raw_tx = hex::encode(bitcoin::consensus::serialize(&x));

//     super::broadcast_recovery_tx_with_mempool(config.mempool_api_url, raw_tx)
//         .await
//         .unwrap();
// }

// #[tokio::test]
// async fn create_tx() {
//     let config = BridgeCliConfig::from_network(bitcoin::Network::Testnet4);

//     let tx = "0200000001114d32a4780fc5ab1da93f7ff3d91b45f280a9eae94736c75f541e645e5391850000000000fdffffff0150c79a3b0000000016001450f49464b82fb00696108904b027276b23cf48ee00000000".to_string();
//     let txx: Transaction = bitcoin::consensus::deserialize(&hex::decode(tx).unwrap()).unwrap();
//     println!("{:?}", txx);

//     // let address = Address::from_str("tb1q2r6fge9c97cqd9ss3yztqfe8dv3u7j8wtq6xc3")
//     //     .unwrap()
//     //     .assume_checked();
//     // let v2_input_txid =
//     //     Txid::from_str("tb1pyfaxye34e5jtmg868yj94359c3qsh2a8luvsxgc9nw4wuvdm2zhsnstdfz")
//     //         .unwrap();
//     // let amount = Amount::from_btc(0.001).unwrap();

//     // let rpc = config.connect_to_bitcoin_rpc().await.unwrap();

//     // let txin = TxIn {
//     //     previous_output: OutPoint {
//     //         txid: v2_input_txid,
//     //         vout: 1,
//     //     },
//     //     ..Default::default()
//     // };
//     // let txout = TxOut {
//     //     value: amount,
//     //     script_pubkey: address.script_pubkey(),
//     // };

//     // let tx = Transaction {
//     //     version: Version::non_standard(3),
//     //     input: vec![txin],
//     //     output: vec![txout],
//     //     lock_time: bitcoin::absolute::LockTime::ZERO,
//     // };
//     // let raw_tx = hex::encode(bitcoin::consensus::serialize(&tx));

//     // super::broadcast_recovery_tx_with_mempool(config.mempool_api_url, raw_tx)
//     //     .await
//     //     .unwrap();
// }
// }
