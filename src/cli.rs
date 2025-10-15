use std::io::Write;
use std::{
    io,
    path::{Path, PathBuf},
    str::FromStr,
};

use bitcoin::{
    Address, Amount, Network, OutPoint,
    address::{NetworkChecked, NetworkUnchecked},
    taproot::Signature,
};
use colored::Colorize;
use eyre::eyre;

use crate::{
    BitcoinAddress, CitreaAddress,
    api_utils::{
        MempoolTx, UtxoInfo, get_current_block_height, get_mempool_txs, get_tx_details, get_utxos,
    },
    backend::{
        backend_deposit_status, backend_withdrawal_status, send_withdrawal_signature_to_operators,
    },
    backup_wallet,
    config::BridgeCliConfig,
    create_encrypted_wallet, deposit,
    errors::BridgeCliError,
    generate_withdrawal_signatures, import_wallet_from_file, import_wallet_from_mnemonic,
    import_wallet_from_private_key,
    secure_display::display_mnemonic_securely,
    secure_types::SecureString,
    structs::{DepositStatusWithVout, TaprootAddressWithPrefix},
    wallet::{
        Purpose, get_mnemonic_from_wallet, get_private_key_from_wallet, get_registry_wallet_set,
        mnemonic::prompt_mnemonic,
        passphrase::{prompt_passphrase, prompt_unlock_passphrase},
        scan_wallet_files,
        wallet_storage::get_storage_dir,
        wallet_utils::{
            WalletValidationMode, ensure_wallet_exists, load_key_with_purpose_check,
            parse_and_validate_imported_wallet, report_integrity_results,
            validate_wallet_availability,
        },
    },
    withdraw::{self, start_withdrawal},
};

use crossterm::cursor::{MoveToColumn, SavePosition};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{
    cursor::MoveUp,
    event::{Event, KeyEventKind, poll, read},
    execute,
};
use std::time::Duration;

pub fn cli_create_wallet(
    network: Network,
    label: String,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Duplicate pre-check before passphrase prompt for better UX
    validate_wallet_availability(Some(&label), None, WalletValidationMode::Label)?;

    let passphrase = prompt_passphrase(true)?;
    let (address, mnemonic) = create_encrypted_wallet(network, label, purpose, passphrase)?;

    let _ = crossterm::terminal::enable_raw_mode();
    print!("\r\n");
    print!(
        "{} Wallet created with address: {}\r\n",
        "SUCCESS".bold(),
        address.address_with_prefix()
    );

    print!("Press any key to continue...\r\n");
    io::stdout().flush().ok();

    // Save the cursor *after* the prompt (the natural place you want to end up)
    execute!(io::stdout(), SavePosition)?;

    loop {
        if poll(Duration::from_secs(60)).unwrap_or(false)
            && let Ok(Event::Key(key_event)) = read()
            && key_event.kind == KeyEventKind::Press
        {
            // Remove the prompt line but keep the cursor where it naturally was
            execute!(
                io::stdout(),
                MoveUp(1),
                Clear(ClearType::CurrentLine),
                MoveToColumn(0),
            )?;
            break;
        }
    }

    let _ = crossterm::terminal::disable_raw_mode();

    display_mnemonic_securely(&mnemonic)?;

    Ok(address)
}

pub fn cli_backup_wallet(
    address_with_prefix: &str,
    destination_path: &str,
) -> Result<(TaprootAddressWithPrefix<NetworkUnchecked>, PathBuf), BridgeCliError> {
    let address = TaprootAddressWithPrefix::from_string_with_prefix_unchecked(address_with_prefix)?;

    // Pre-check to ensure wallet exists before prompting for passphrase for better UX
    ensure_wallet_exists(&address)?;

    let dest_path = Path::new(destination_path);
    let final_dest = backup_wallet(&address, dest_path)?;

    Ok((address, final_dest))
}

pub fn cli_import_wallet_from_mnemonic(
    network: Network,
    label: &str,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Duplicate pre-check before mnemonic prompt for better UX
    validate_wallet_availability(Some(label), None, WalletValidationMode::Label)?;
    println!("{}", "Import Wallet with Mnemonic".bold());

    let mnemonic = prompt_mnemonic()?;

    import_wallet_from_mnemonic(network, label, purpose, mnemonic)
}

pub fn cli_verify_wallet_integrity() -> Result<(), BridgeCliError> {
    let storage_dir = get_storage_dir()?;

    println!("{}", "Verifying Wallet Integrity".bold());
    println!("Storage directory: {}", storage_dir.display());
    println!();

    // Load registered wallets from wallets.json
    let registry_wallets = get_registry_wallet_set()?;

    // Scan for actual wallet files in storage directory
    let file_wallets = scan_wallet_files()?;

    // Report integrity results
    report_integrity_results(&registry_wallets, &file_wallets);

    Ok(())
}

pub fn cli_import_wallet_from_file(
    file_path: &str,
    label: Option<&str>,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    let file_path = Path::new(file_path);

    // Duplicate pre-check before passphrase prompt for better UX
    parse_and_validate_imported_wallet(file_path, label)?;

    let passphrase = prompt_unlock_passphrase()?;
    import_wallet_from_file(file_path, label, passphrase)
}

pub fn cli_import_wallet_from_private_key(
    network: Network,
    label: &str,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Duplicate pre-check before passphrase prompt for better UX
    validate_wallet_availability(Some(label), None, WalletValidationMode::Label)?;

    let private_key_input = rpassword::prompt_password("Enter your private key (hex format): ")
        .map_err(|e| BridgeCliError::Eyre(eyre!("Failed to read private key: {}", e)))?;

    let secure_private_key = SecureString::init_with(|| private_key_input);

    let passphrase = prompt_passphrase(true)?;

    import_wallet_from_private_key(network, label, purpose, secure_private_key, passphrase)
}

/// Show mnemonic securely for a wallet
pub fn cli_show_mnemonic(
    address: &TaprootAddressWithPrefix<bitcoin::address::NetworkUnchecked>,
) -> Result<(), BridgeCliError> {
    // Pre-check to ensure wallet exists before prompting for passphrase for better UX
    ensure_wallet_exists(address)?;
    let passphrase = prompt_unlock_passphrase()?;
    let mnemonic = get_mnemonic_from_wallet(address, &passphrase)?;
    crate::secure_display::display_mnemonic_securely(&mnemonic)?;
    Ok(())
}

/// Show private key securely for a wallet
pub fn cli_show_private_key(
    address: &TaprootAddressWithPrefix<bitcoin::address::NetworkUnchecked>,
) -> Result<(), BridgeCliError> {
    // Pre-check to ensure wallet exists before prompting for passphrase for better UX
    ensure_wallet_exists(address)?;
    let passphrase = prompt_unlock_passphrase()?;
    let private_key = get_private_key_from_wallet(address, &passphrase)?;
    crate::secure_display::display_private_key_securely(&private_key)?;
    Ok(())
}

pub async fn deposit_status(
    taproot_address: Address,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    let mut utxos = match get_utxos(&taproot_address, config).await {
        Ok(utxos) => utxos,
        Err(e) => {
            eprintln!("ERROR Failed to fetch UTXOs from mempool.space: {}", e);
            vec![]
        }
    };

    utxos.sort_by_key(|utxo| utxo.block_height.unwrap_or(u64::MAX));
    let deposits_with_incorrect_amount: Vec<&UtxoInfo> = utxos
        .iter()
        .filter(|utxo| utxo.value != config.bridge_amount)
        .collect();

    if !deposits_with_incorrect_amount.is_empty() {
        println!(
            "{} Deposits with incorrect amount for address {}:",
            "WARNING".bold(),
            taproot_address
        );
    }

    let current_block_height = get_current_block_height(config).await?;

    let refund_info = |block_height: Option<u64>, move_tx_on_chain: bool| {
        let refund_in_blocks = block_height.and_then(|h| {
            h.checked_add(config.user_takes_after)
                .map(|target| target.saturating_sub(current_block_height))
        });
        if move_tx_on_chain {
            match refund_in_blocks {
                Some(0) => "\n  You can refund your deposit now using 'deposit create-signed-recovery-tx' subcommand.".to_string(),
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

    for utxo in &deposits_with_incorrect_amount {
        let refund_msg = refund_info(utxo.block_height, true);
        print_incorrect_deposit(utxo, &refund_msg, &block_display(utxo.block_height));
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

    let deposit_statuses_message = if deposit_statuses_backend.is_empty() {
        format!(
            "{} No deposits found for address {}",
            "INFO".bold(),
            taproot_address.to_string().bold()
        )
    } else {
        format!(
            "{} Deposit status(es) for address {}:",
            "INFO".bold(),
            taproot_address
        )
    };

    println!("{}", deposit_statuses_message);

    for status in &deposit_statuses_backend {
        let corresponding_utxo = utxos
            .iter()
            .find(|utxo| utxo.txid.to_string() == status.txid);
        let block_height = corresponding_utxo.and_then(|u| u.block_height);
        let tx_details = get_tx_details(&bitcoin::Txid::from_str(&status.txid)?, config).await;

        let (vout, found) = match tx_details {
            Ok((tx, _, _)) => {
                match tx
                    .output
                    .iter()
                    .position(|o| o.script_pubkey == taproot_address.script_pubkey())
                {
                    Some(v) => (v as u32, true),
                    None => {
                        return Err(BridgeCliError::Eyre(eyre!(
                            "Could not find vout for deposit txid {} and address {}",
                            status.txid,
                            taproot_address
                        )));
                    }
                }
            }
            Err(_) => (0, false),
        };

        let deposit_status_with_vout = DepositStatusWithVout {
            deposit_status: status,
            vout: if found { Some(vout) } else { None },
        };

        let refund_msg = refund_info(block_height, status.move_txid.is_empty());
        println!("{} {}", deposit_status_with_vout, refund_msg);
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
    destination_addr: &BitcoinAddress,
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
        destination_addr: destination_addr.clone(),
        fee_rate: Some(fee_rate),
        amount: Some(amount),
    };

    let tx = deposit::create_signed_recovery_tx(recovery_params, config, keypair)?;

    let raw_tx = hex::encode(bitcoin::consensus::serialize(&tx));
    println!("Raw transaction: {raw_tx}");
    println!();
    println!("Now you can broadcast the transaction using your preferred method.");

    Ok(())
}

pub async fn withdrawal_status(
    withdrawal_utxo: OutPoint,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    let withdrawal_statuses = backend_withdrawal_status(withdrawal_utxo, config).await?;
    if withdrawal_statuses.is_empty() {
        println!(
            "{} No withdrawals found for OutPoint {}",
            "INFO".bold(),
            withdrawal_utxo.to_string().bold()
        );
        return Ok(());
    }

    println!(
        "{} Withdrawal status(es) for withdrawal OutPoint {}: \n",
        "INFO".bold(),
        withdrawal_utxo
    );

    for (i, status) in withdrawal_statuses.iter().enumerate() {
        println!("{}. {}", i + 1, status);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn send_withdrawal_signature(
    signer_address: &str,
    destination_address: &str,
    withdrawal_utxo_outpoint: &str,
    amount: u64,
    signature: &str,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    let withdrawal_outpoint = OutPoint::from_str(withdrawal_utxo_outpoint)?;
    send_withdrawal_signature_to_operators(
        signer_address,
        destination_address,
        withdrawal_outpoint,
        signature,
        config,
        amount,
    )
    .await?;
    Ok(())
}

fn print_incorrect_deposit(utxo: &UtxoInfo, refund_message: &str, block_display: &str) {
    println!(
        "\nIncorrect Deposit\n  TxID:        {}\n  Value:       {}\n  Block:       {}{}",
        utxo.txid, utxo.value, block_display, refund_message
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

pub async fn cli_get_deposit_address(
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    config: &BridgeCliConfig,
) -> Result<BitcoinAddress, BridgeCliError> {
    let deposit_address =
        deposit::get_deposit_address(citrea_address, recovery_taproot_address, config).await?;
    Ok(deposit_address)
}

pub async fn cli_start_withdrawal(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    destination_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    start_withdrawal(signer_address, destination_address, config)?;
    Ok(())
}

pub async fn cli_scan_withdrawals(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    destination_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    let utxos = withdraw::scan_withdrawal(signer_address, destination_address, config).await;

    let mut utxos =
        utxos.inspect_err(|e| eprintln!("{} Failed to scan withdrawals: {}", "ERROR".bold(), e))?;

    utxos.sort_by_key(|(outpoint, _)| outpoint.txid);

    let utxos_with_wrong_amount: Vec<_> = utxos
        .iter()
        .filter(|(_, amount)| *amount != config.dust_utxo_amount)
        .collect();

    if !utxos_with_wrong_amount.is_empty() {
        eprintln!(
            "{} The following UTXOs have amounts different than {} BTC. They will be ignored for withdrawal operations.",
            config.dust_utxo_amount.to_btc(),
            "WARNING".bold()
        );
        for (outpoint, amount) in utxos_with_wrong_amount {
            eprintln!(" - OutPoint: {}, Amount: {}", outpoint, amount);
        }
        eprintln!(
            "Please ensure you send exactly {} BTC to the signer address for each withdrawal operation.",
            config.dust_utxo_amount.to_btc()
        );

        // sleep for 2 seconds to ensure user sees the warning
        std::thread::sleep(std::time::Duration::from_secs(2));

        println!();
    }

    utxos.retain(|(_, amount)| *amount == config.dust_utxo_amount);

    // Now for valid UTXOs, check if the backend already has a withdrawal for them
    // Ask status of each UTXO
    let mut available_utxos = Vec::new();
    for (outpoint, amount) in utxos.iter() {
        if backend_withdrawal_status(*outpoint, config)
            .await?
            .is_empty()
        {
            tracing::debug!("No withdrawal found for UTXO: {}", outpoint);
            // If error, assume no withdrawal exists for this UTXO
            available_utxos.push((outpoint, amount));
        }
    }

    if available_utxos.is_empty() {
        eprintln!(
            "No UTXOs found. Please send 0.00000{} BTC first using 'withdrawal start' command",
            config.dust_utxo_amount.to_sat()
        );
    } else {
        let print_withdrawal_cmd = |outpoint: &_| {
            println!(
                "clementine-cli withdraw generate-withdrawal-signatures --network {} {} {} {}",
                config.network,
                &signer_address.address_with_prefix(),
                destination_address,
                outpoint,
            );
        };
        if available_utxos.len() == 1 {
            println!("Run:");
            let (outpoint, _) = &available_utxos[0];
            print_withdrawal_cmd(outpoint);
        } else {
            println!(
                "{} Multiple UTXOs found, we advise to use one UTXO for one withdrawal operation",
                "WARNING".bold()
            );
            println!(
                "{} For your security: Use a unique signer address for each withdrawal.",
                "IMPORTANT NOTICE!".bold()
            );
            println!("Run one of these:");
            for (outpoint, _) in available_utxos.iter() {
                print_withdrawal_cmd(outpoint);
                println!()
            }
        }
    }

    Ok(())
}

pub fn cli_generate_withdrawal_signatures(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    destination_address: &BitcoinAddress,
    withdrawal_utxo: &OutPoint,
    optimistic_withdrawal_amount: &Amount,
    operator_withdrawal_amount: &Amount,
    config: &BridgeCliConfig,
) -> Result<(Signature, Signature), BridgeCliError> {
    let keypair = crate::wallet::wallet_utils::load_key_with_purpose_check(
        signer_address,
        Purpose::Withdrawal,
    )?;

    generate_withdrawal_signatures(
        keypair,
        signer_address,
        destination_address,
        withdrawal_utxo,
        optimistic_withdrawal_amount,
        operator_withdrawal_amount,
        config,
    )
}
