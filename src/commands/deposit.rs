use crate::{
    BitcoinAddress, CitreaAddress,
    api_utils::{
        MempoolTx, get_current_block_height, get_mempool_txs, utxos_from_mempool_space_api,
    },
    backend::backend_deposit_status,
    bitcoin_utils::Utxo,
    config::BridgeCliConfig,
    deposit,
    errors::BridgeCliError,
    structs::TaprootAddressWithPrefix,
    wallet::{
        Purpose,
        wallet_utils::{ensure_wallet_exists, load_key_with_purpose_check},
    },
};
use bitcoin::{Address, Amount, OutPoint};
use colored::Colorize;

fn print_incorrect_deposit(
    utxo: &Utxo,
    refund_message: &str,
    block_display: &str,
    is_confirmed_display: &str,
) {
    println!(
        "\nIncorrect Deposit\n  TxID:        {}\n  Value:       {}\n  Block:       {}\n  UTXO Status: {}{}",
        utxo.txid,
        Amount::from_sat(utxo.value),
        block_display,
        is_confirmed_display,
        refund_message
    );
}

fn print_mempool_tx(address: &BitcoinAddress, tx: &MempoolTx) {
    for out in &tx.vout {
        if let Some(addr_str) = out
            .get("scriptpubkey_address")
            .and_then(|addr| addr.as_str())
            && addr_str == address.to_string()
        {
            let value = out.get("value").and_then(|v| v.as_u64()).unwrap_or(0);
            println!("\nDeposit in Mempool");
            println!(
                "  TxID:       {}\n  Value:      {}",
                tx.txid,
                Amount::from_sat(value)
            );
        };
    }
}

pub async fn deposit_status(
    taproot_address: Address,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    let mut utxos = match utxos_from_mempool_space_api(&taproot_address, config).await {
        Ok(utxos) => utxos,
        Err(e) => {
            eprintln!("ERROR Failed to fetch UTXOs from mempool.space: {}", e);
            vec![]
        }
    };
    utxos.sort_by_key(|utxo| utxo.status.block_height.unwrap_or(u64::MAX));
    let deposits_with_incorrect_amount: Vec<&Utxo> = utxos
        .iter()
        .filter(|utxo| utxo.value != config.bridge_amount.to_sat())
        .collect();

    if !deposits_with_incorrect_amount.is_empty() {
        println!(
            "{} Deposits with incorrect amount for address {}:",
            "WARNING".bold(),
            taproot_address
        );
    }

    let current_block_height = get_current_block_height(config).await?;

    let refund_info = |block_height: Option<u64>, move_txid_empty: bool| {
        let refund_in_blocks = block_height.and_then(|h| {
            h.checked_add(config.user_takes_after)
                .map(|target| target.saturating_sub(current_block_height))
        });
        if move_txid_empty {
            match refund_in_blocks {
                Some(0) => "\n  You can refund your deposit now using 'create-signed-recovery-tx' subcommand.".to_string(),
                Some(blocks) => format!("\n  Refund in (approx.) blocks: {}", blocks),
                None => "\n  Refund information not available.".to_string(),
            }
        } else {
            String::new()
        }
    };

    let block_display = |block_height: Option<u64>| {
        block_height
            .map(|h| h.to_string())
            .unwrap_or_else(|| "N/A".to_string())
    };

    let is_confirmed_display = |confirmed: bool| {
        if confirmed {
            "Confirmed"
        } else {
            "Unconfirmed"
        }
    };

    for utxo in &deposits_with_incorrect_amount {
        let refund_msg = refund_info(utxo.status.block_height, true);
        print_incorrect_deposit(
            utxo,
            &refund_msg,
            &block_display(utxo.status.block_height),
            is_confirmed_display(utxo.status.confirmed),
        );
    }

    if !deposits_with_incorrect_amount.is_empty() {
        println!();
    }

    let deposit_statuses_backend = match backend_deposit_status(&taproot_address, config).await {
        Ok(statuses) => statuses,
        Err(e) => {
            eprintln!("ERROR Failed to fetch deposit statuses from backend: {}", e);
            vec![]
        }
    };
    if deposit_statuses_backend.is_empty() {
        println!(
            "{} No deposits found for address {}",
            "INFO".bold(),
            taproot_address.to_string().bold()
        );
        return Ok(());
    }
    println!(
        "{} Deposit status(es) for address {}:",
        "INFO".bold(),
        taproot_address
    );
    for status in &deposit_statuses_backend {
        let corresponding_utxo = utxos.iter().find(|utxo| utxo.txid == status.txid);
        let block_height = corresponding_utxo.and_then(|u| u.status.block_height);
        let refund_msg = refund_info(block_height, status.move_txid.is_empty());
        println!("{} {}", status, refund_msg);
    }

    let mempool_txs = match get_mempool_txs(&taproot_address, config).await {
        Ok(txs) => txs,
        Err(e) => {
            eprintln!("ERROR Failed to fetch mempool transactions: {}", e);
            vec![]
        }
    };

    if !mempool_txs.is_empty() {
        println!(
            "\n{} Deposit transactions in mempool for address {}:",
            "INFO".bold(),
            taproot_address
        );

        for tx in &mempool_txs {
            print_mempool_tx(&taproot_address, tx);
        }
    }

    Ok(())
}

pub async fn deposit_create_signed_recovery_tx(
    citrea_addr: &CitreaAddress,
    recovery_taproot_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    outpoint: &OutPoint,
    claim_addr: &BitcoinAddress,
    fee_rate: u64,
    amount: f64,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    ensure_wallet_exists(recovery_taproot_address)?;

    let keypair = load_key_with_purpose_check(recovery_taproot_address, Purpose::Deposit)?;

    let recovery_params = deposit::RecoveryTxParams {
        citrea_addr: *citrea_addr,
        recovery_taproot_address: recovery_taproot_address.clone(),
        outpoint: *outpoint,
        claim_addr: claim_addr.clone(),
        fee_rate: Some(fee_rate),
        amount: Some(amount),
    };

    let tx = deposit::create_signed_recovery_tx(recovery_params, config, keypair)?;

    let raw_tx = hex::encode(bitcoin::consensus::serialize(&tx));
    println!("Raw transaction: {raw_tx}");

    Ok(())
}
