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
use crate::{BitcoinAddress, CitreaAddress};
use bitcoin::{Amount, FeeRate, OutPoint, Transaction, Txid};
use eyre::Result;

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
    fee_rate: u64,
    amount: f64,
    config: &BridgeCliConfig,
) -> Result<Transaction, BridgeCliError> {
    // Always prompt for passphrase for maximum security
    let secure_passphrase = prompt_unlock_passphrase()?;

    if recovery_taproot_address.purpose != Purpose::Deposit {
        return Err(BridgeCliError::PurposeMismatch {
            expected: Purpose::Deposit,
            found: recovery_taproot_address.purpose,
        });
    }

    let keypair = load_key(recovery_taproot_address, &secure_passphrase)?;

    // Convert BTC amount to satoshis if provided
    let deposit_amount = Amount::from_btc(amount)?;

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
