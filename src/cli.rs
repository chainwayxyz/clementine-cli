use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use bitcoin::{
    Address, Network, OutPoint,
    address::{NetworkChecked, NetworkUnchecked},
};
use colored::Colorize;
use eyre::eyre;

use crate::{
    BitcoinAddress, CitreaAddress,
    backend::{
        backend_deposit_status, backend_withdrawal_status, send_withdrawal_signatures_to_operators,
    },
    backup_wallet,
    bitcoin_utils::{Utxo, get_current_block_height, utxos_from_mempool_space_api},
    config::BridgeCliConfig,
    create_encrypted_wallet, deposit,
    errors::BridgeCliError,
    import_wallet_from_file, import_wallet_from_mnemonic, import_wallet_from_private_key,
    secure_display::display_mnemonic_securely,
    structs::{SecureString, TaprootAddressWithPrefix},
    wallet::{
        Purpose, get_mnemonic_from_wallet, get_private_key_from_wallet, get_registry_wallet_set,
        mnemonic::prompt_mnemonic,
        passphrase::{prompt_passphrase, prompt_unlock_passphrase},
        scan_wallet_files,
        wallet_storage::get_storage_dir,
        wallet_utils::{
            WalletValidationMode, ensure_wallet_exists, parse_and_validate_imported_wallet,
            report_integrity_results, validate_wallet_availability,
        },
    },
    withdrawal::start_withdrawal,
};

pub fn cli_create_wallet(
    network: Network,
    label: String,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Duplicate pre-check before passphrase prompt for better UX
    validate_wallet_availability(Some(&label), None, WalletValidationMode::Label)?;

    let passphrase = prompt_passphrase(true)?;
    let (address, mnemonic) = create_encrypted_wallet(network, label, purpose, passphrase)?;

    println!(
        "{} Wallet created with address: {}",
        "SUCCESS".bold(),
        address.address_with_prefix()
    );

    // Sleep for 2 seconds to allow user to see the success message before showing mnemonic
    std::thread::sleep(std::time::Duration::from_secs(2));

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
    println!("{}", "Import Wallet with Mnemonic".blue().bold());

    let mnemonic = prompt_mnemonic()?;

    import_wallet_from_mnemonic(network, label, purpose, mnemonic)
}

pub fn cli_verify_wallet_integrity() -> Result<(), BridgeCliError> {
    let storage_dir = get_storage_dir()?;

    println!("{}", "Verifying Wallet Integrity".blue().bold());
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
    let mut utxos = utxos_from_mempool_space_api(&taproot_address, config).await?;
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
                None => "Refund information not available.".to_string(),
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

    let deposit_statuses_backend = backend_deposit_status(&taproot_address, config).await?;
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
    Ok(())
}

pub async fn withdrawal_status(
    withdrawal_index: u32,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    let withdrawal_statuses = backend_withdrawal_status(withdrawal_index, config).await?;
    if withdrawal_statuses.is_empty() {
        println!(
            "{} No withdrawals found for index {}",
            "INFO".yellow().bold(),
            withdrawal_index.to_string().blue().bold()
        );
        return Ok(());
    }

    println!(
        "{} Withdrawal status(es) for index {}: \n",
        "INFO".blue().bold(),
        withdrawal_index
    );

    for (i, status) in withdrawal_statuses.iter().enumerate() {
        println!("{}. {}", i + 1, status);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn send_withdrawal_signatures(
    signer_address: &str,
    withdrawal_address: &str,
    withdrawal_utxo_outpoint: &str,
    amount: f64,
    signature: &str,
    config: &BridgeCliConfig,
    withdrawal_index: u32,
) -> Result<(), BridgeCliError> {
    let withdrawal_outpoint = OutPoint::from_str(withdrawal_utxo_outpoint)?;
    send_withdrawal_signatures_to_operators(
        signer_address,
        withdrawal_address,
        withdrawal_outpoint,
        withdrawal_index,
        signature,
        config,
        amount,
    )
    .await?;
    Ok(())
}

fn print_incorrect_deposit(
    utxo: &Utxo,
    refund_message: &str,
    block_display: &str,
    is_confirmed_display: &str,
) {
    println!(
        "\nIncorrect Deposit\n  TxID:        {}\n  Value:       {}\n  Block:       {}\n  UTXO Status: {}{}",
        utxo.txid, utxo.value, block_display, is_confirmed_display, refund_message
    );
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
    claim_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    start_withdrawal(signer_address, claim_address, config)?;
    Ok(())
}
