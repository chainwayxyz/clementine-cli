// Deposit-related commands and logic for Clementine CLI

use crate::backend::create_deposit_account;
use crate::bitcoin_utils::calculate_deposit_address;
use crate::bitcoin_utils::sign_recovery_tx as utils_sign_recovery_tx;
use crate::config::BridgeCliConfig;
use crate::errors::BridgeCliError;
use crate::parameters::get_citrea_deposit_params;
use crate::structs::TaprootAddressWithPrefix;
use crate::wallet::Purpose;
use crate::wallet::passphrase::prompt_unlock_passphrase;
use crate::wallet::wallet_utils::load_key;
use crate::withdrawal::{get_tx_details, get_txout_details};
use crate::{BitcoinAddress, CitreaAddress, parse_citrea_address};
use bitcoin::Address;
use bitcoin::consensus::deserialize;
use bitcoin::{Amount, FeeRate, OutPoint, Transaction, Txid};
use colored::*;
use eyre::Result;
use std::str::FromStr;

/// Get deposit address from backend
pub async fn get_deposit_address(
    citrea_address: &str,
    recovery_taproot_address: &str,
    config: &BridgeCliConfig,
) -> Result<Address, BridgeCliError> {
    let citrea_address: CitreaAddress = parse_citrea_address(citrea_address)?;
    tracing::debug!(
        "{} {}",
        "CITREA_ADDRESS (checksummed)".green().bold(),
        citrea_address,
    );
    let recovery_taproot_address = TaprootAddressWithPrefix::from_string_with_prefix(recovery_taproot_address, config.network)?;

    if recovery_taproot_address.purpose != Purpose::Deposit {
        return Err(BridgeCliError::PurposeMismatch(
            Purpose::Deposit,
            recovery_taproot_address.purpose,
        ));
    }

    // Call backend to create deposit account
    let deposit_address =
        create_deposit_account(&citrea_address, &recovery_taproot_address.address, config).await?;

    tracing::debug!("{} {}", "DEPOSIT_ADDRESS".green().bold(), deposit_address);

    let (calculated_deposit_address, _) =
        calculate_deposit_address(&citrea_address, &recovery_taproot_address.address, config)?;

    assert_eq!(deposit_address, calculated_deposit_address);

    Ok(calculated_deposit_address)
}

pub async fn get_deposit_params(
    move_to_vault_txid: &str,
    config: &BridgeCliConfig,
) -> Result<Vec<u8>, BridgeCliError> {
    let move_to_vault_txid = Txid::from_str(move_to_vault_txid)?;
    // 2. Get the prepare tx details
    let (move_to_vault_tx, move_to_vault_block, move_to_vault_block_height) =
        get_tx_details(&move_to_vault_txid, config).await?;

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

#[allow(clippy::too_many_arguments)]
pub fn sign_recovery_tx(
    citrea_address: &str,
    recovery_taproot_address: &str,
    deposit_txid: &str,
    deposit_vout: u32,
    claim_address: &str,
    fee_rate: Option<u64>,
    amount: Option<f64>,
    config: &BridgeCliConfig,
) -> Result<Transaction, BridgeCliError> {
    let citrea_addr: CitreaAddress = parse_citrea_address(citrea_address)?;
    let claim_addr = BitcoinAddress::from_str(claim_address)?.require_network(config.network)?;
    let txid = Txid::from_str(deposit_txid)?;
    let outpoint = OutPoint {
        txid,
        vout: deposit_vout,
    };
    // Always prompt for passphrase for maximum security
    println!("Please enter the passphrase for the recovery key:");
    let secure_passphrase = prompt_unlock_passphrase()?;

    let recovery_taproot_address = TaprootAddressWithPrefix::from_string_with_prefix(
        recovery_taproot_address,
        config.network,
    )?;

    if recovery_taproot_address.purpose != Purpose::Deposit {
        return Err(BridgeCliError::PurposeMismatch(
            Purpose::Deposit,
            recovery_taproot_address.purpose,
        ));
    }

    let keypair = load_key(&recovery_taproot_address, &secure_passphrase)?;

    // Convert BTC amount to satoshis if provided
    let deposit_amount = match amount {
        Some(btc) => Some(Amount::from_btc(btc)?),
        None => None,
    };

    let fee_rate_opt = fee_rate.map(FeeRate::from_sat_per_vb_unchecked);
    let signed_tx = utils_sign_recovery_tx(
        &keypair,
        &citrea_addr,
        &recovery_taproot_address.address,
        &outpoint,
        deposit_amount,
        &claim_addr,
        fee_rate_opt,
        config,
    )?;

    Ok(signed_tx)
}

#[allow(clippy::too_many_arguments)]
pub fn verify_recovery_tx(
    recovery_tx: &str,
    citrea_address: &str,
    recovery_taproot_address: &str,
    amount: Option<f64>,
    config: &BridgeCliConfig,
) -> Result<(Txid, BitcoinAddress, Amount), BridgeCliError> {
    let recovery_tx: Transaction = deserialize(&hex::decode(recovery_tx)?)?;

    let recovery_taproot_address = TaprootAddressWithPrefix::from_string_with_prefix(recovery_taproot_address, config.network)?;

    if recovery_taproot_address.purpose != Purpose::Deposit {
        return Err(BridgeCliError::PurposeMismatch(
            Purpose::Deposit,
            recovery_taproot_address.purpose,
        ));
    }

    let (txid, address, amount) = crate::bitcoin_utils::verify_recovery_tx(
        &recovery_tx,
        &parse_citrea_address(citrea_address)?,
        &recovery_taproot_address.address,
        amount.map(|amount| Amount::from_btc(amount).unwrap()),
        config,
    )?;

    println!(
        "{} Recovery transaction verification successful!",
        "SUCCESS".green().bold()
    );
    println!("{} {}", "Output address:".blue().bold(), address);
    println!("{} {} BTC", "Output amount:".blue().bold(), amount.to_btc());
    println!(
        "\n{} This transaction can be broadcast after 200 blocks from {}",
        "NOTE:".yellow().bold(),
        txid
    );

    Ok((txid, address, amount))
}
