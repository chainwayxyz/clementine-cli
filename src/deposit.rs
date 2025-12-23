// Deposit-related commands and logic for Clementine CLI

use crate::api_utils::{get_tx_details, get_txout_details};
use crate::backend::create_deposit_account;
use crate::bitcoin_utils::{calculate_deposit_address, convert_btc_to_amount};
use crate::config::BridgeCliConfig;
use crate::errors::BridgeCliError;
use crate::parameters::get_citrea_deposit_params;
use crate::secure_types::SecureKeypair;
use crate::structs::TaprootAddressWithPrefix;
use crate::wallet::Purpose;
use crate::wallet::wallet_utils::ensure_wallet_exists;
use crate::wallet::wallet_utils::validate_address_purpose;
use crate::{BitcoinAddress, CitreaAddress, get_clementine_home_dir};
use bitcoin::address::NetworkUnchecked;
use bitcoin::{Amount, FeeRate, Network, OutPoint, Transaction, Txid};
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

/// Parameters for creating a signed recovery transaction
pub struct RecoveryTxParams {
    pub citrea_addr: CitreaAddress,
    pub recovery_taproot_address: TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    pub outpoint: OutPoint,
    pub destination_addr: BitcoinAddress,
    pub fee_rate: Option<u64>,
    pub amount: Option<f64>,
}

/// Parameters for verifying a recovery transaction
#[derive(Debug)]
pub struct VerifyRecoveryTxParams {
    pub recovery_tx: Transaction,
    pub citrea_address: CitreaAddress,
    pub recovery_taproot_address: TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    pub amount: Option<f64>,
}

pub(crate) enum DepositStatusEnum {
    New,
    InProgress,
    MoveTxSent,
    Completed,
    Unknown,
}

/// Data structure representing a deposit address mapping.
///
/// Associates a Bitcoin deposit address with its recovery taproot address,
/// Citrea address, and network information for storage and retrieval.
#[derive(Debug, Clone)]
pub struct DepositData {
    pub deposit_address: BitcoinAddress,
    pub recovery_taproot_address: TaprootAddressWithPrefix<NetworkUnchecked>,
    pub citrea_address: CitreaAddress,
    pub network: Network,
}

const DEPOSIT_ADDRESS_STORAGE_FILE: &str = "deposit_addresses.json";

/// A single deposit address entry stored in the JSON file.
///
/// Contains the deposit address and associated Citrea address as strings.
/// Multiple entries can exist for each recovery taproot address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDepositEntry {
    pub deposit_address: String,
    pub citrea_address: String,
}

/// Deposit details for a specific recovery taproot address.
///
/// Contains the network, list of deposit entries, and creation timestamp.
/// This structure is stored in the JSON file keyed by the recovery taproot address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositDetails {
    pub network: Network,
    pub entries: Vec<StoredDepositEntry>,
    pub created_at: u64,
}

type StoredDepositMap = HashMap<String, DepositDetails>;

impl DepositStatusEnum {
    pub(crate) fn from_status(status: &str) -> Self {
        match status {
            "new" => DepositStatusEnum::New,
            "minted" => DepositStatusEnum::Completed,
            "flushing_initiating" | "flushing_initiated" | "flushing_broadcasting" => {
                DepositStatusEnum::InProgress
            }
            "sent" => DepositStatusEnum::MoveTxSent,
            _ => DepositStatusEnum::Unknown,
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            DepositStatusEnum::New => "New".to_string(),
            DepositStatusEnum::InProgress => "In Progress".to_string(),
            DepositStatusEnum::Completed => "Completed".to_string(),
            DepositStatusEnum::MoveTxSent => "Move To Vault Transaction Sent".to_string(),
            DepositStatusEnum::Unknown => "Unknown".to_string(),
        }
    }

    /// Returns the progress position (current step, total steps)
    pub fn progress(&self) -> (usize, usize) {
        match self {
            DepositStatusEnum::New => (1, 4),
            DepositStatusEnum::InProgress => (2, 4),
            DepositStatusEnum::MoveTxSent => (3, 4),
            DepositStatusEnum::Completed => (4, 4),
            DepositStatusEnum::Unknown => (0, 4),
        }
    }

    /// Returns a description of the current step
    pub fn step_description(&self) -> &str {
        match self {
            DepositStatusEnum::New => "Deposit detected on Bitcoin network",
            DepositStatusEnum::InProgress => "The deposit is being processed",
            DepositStatusEnum::MoveTxSent => {
                "Move transaction broadcasted, waiting for confirmation and minting"
            }
            DepositStatusEnum::Completed => "Funds minted on Citrea network",
            DepositStatusEnum::Unknown => "Status unknown",
        }
    }
}

fn store_deposit_address(deposit_data: &DepositData) -> Result<(), BridgeCliError> {
    let storage_path = get_clementine_home_dir()?.join(DEPOSIT_ADDRESS_STORAGE_FILE);

    let mut map = if storage_path.exists() {
        let contents = fs::read_to_string(&storage_path)?;
        if contents.trim().is_empty() {
            StoredDepositMap::new()
        } else {
            serde_json::from_str::<StoredDepositMap>(&contents)?
        }
    } else {
        StoredDepositMap::new()
    };

    let key = deposit_data.recovery_taproot_address.address_with_prefix();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_else(|e| {
            tracing::warn!("System time error, using 0 as timestamp: {}", e);
            0
        });

    let entry = StoredDepositEntry {
        deposit_address: deposit_data.deposit_address.to_string(),
        citrea_address: deposit_data.citrea_address.to_string(),
    };

    let details = map.entry(key).or_insert_with(|| DepositDetails {
        network: deposit_data.network,
        entries: Vec::new(),
        created_at: now,
    });

    if details.network != deposit_data.network {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Network mismatch for recovery taproot address '{}': existing network '{:?}', new network '{:?}'",
            deposit_data.recovery_taproot_address.address_with_prefix(),
            details.network,
            deposit_data.network
        )));
    }

    let is_duplicate = details.entries.iter().any(|e| {
        e.deposit_address == entry.deposit_address && e.citrea_address == entry.citrea_address
    });

    if !is_duplicate {
        details.entries.push(entry);
    }

    let tmp_path = storage_path.with_file_name(format!(
        "{}.tmp",
        storage_path
            .file_name()
            .expect("Storage path has a file name")
            .to_string_lossy()
    ));

    let json = serde_json::to_string_pretty(&map)?;

    {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
    }

    fs::rename(&tmp_path, &storage_path).map_err(|e| {
        let _ = fs::remove_file(tmp_path);
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to rename temp deposit address storage file: {}",
            e
        ))
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&storage_path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&storage_path, perms)?;
    }

    Ok(())
}

pub fn get_stored_deposit_recovery_taproot_addresses()
-> Result<Vec<(String, Network)>, BridgeCliError> {
    let storage_path = get_clementine_home_dir()?.join(DEPOSIT_ADDRESS_STORAGE_FILE);

    if !storage_path.exists() {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(&storage_path)?;
    if contents.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut pairs: Vec<(String, DepositDetails)> =
        serde_json::from_str::<StoredDepositMap>(&contents)?
            .into_iter()
            .collect();

    // Sort deterministically by creation time (and then by address for tie-breaker)
    pairs.sort_by(|(a_addr, a_details), (b_addr, b_details)| {
        a_details
            .created_at
            .cmp(&b_details.created_at)
            .then_with(|| a_addr.cmp(b_addr))
    });

    Ok(pairs
        .into_iter()
        .map(|(addr, details)| (addr, details.network))
        .collect())
}

/// Retrieves deposit details for a specific recovery taproot address.
///
/// # Arguments
/// * `recovery_taproot_address` - The recovery taproot address to look up (must have deposit purpose)
///
/// # Returns
/// - `Ok(Some(DepositDetails))`: If the address has stored deposit data
/// - `Ok(None)`: If no data exists for the address
/// - `Err(BridgeCliError)`: If validation fails or file operations fail
pub fn get_stored_deposit_addresses_for_recovery_taproot_address(
    recovery_taproot_address: &TaprootAddressWithPrefix<NetworkUnchecked>,
) -> Result<Option<DepositDetails>, BridgeCliError> {
    crate::wallet::wallet_utils::validate_address_purpose(
        recovery_taproot_address,
        Purpose::Deposit,
    )?;

    let storage_path = get_clementine_home_dir()?.join(DEPOSIT_ADDRESS_STORAGE_FILE);

    if !storage_path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&storage_path)?;

    if contents.trim().is_empty() {
        return Ok(None);
    }

    let map: StoredDepositMap = serde_json::from_str::<StoredDepositMap>(&contents)?;

    let key = recovery_taproot_address.address_with_prefix();

    if let Some(details) = map.get(&key) {
        let deposit_data = details.clone();
        Ok(Some(deposit_data))
    } else {
        Ok(None)
    }
}

/// Get deposit address from backend
pub async fn get_deposit_address(
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    config: &BridgeCliConfig,
) -> Result<BitcoinAddress, BridgeCliError> {
    crate::wallet::wallet_utils::validate_address_purpose(
        recovery_taproot_address,
        Purpose::Deposit,
    )?;

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

    let deposit_data = DepositData {
        deposit_address: calculated_deposit_address.clone(),
        recovery_taproot_address: recovery_taproot_address.into(),
        citrea_address: *citrea_address,
        network: config.network,
    };

    store_deposit_address(&deposit_data).map_err(|e| {
        tracing::error!("Failed to store deposit address: {}", e);
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to store deposit address for recovery taproot address '{}'",
            recovery_taproot_address.address_with_prefix()
        ))
    })?;

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
pub async fn create_signed_recovery_tx(
    params: RecoveryTxParams,
    config: &BridgeCliConfig,
    keypair: SecureKeypair,
) -> Result<Transaction, BridgeCliError> {
    ensure_wallet_exists(&params.recovery_taproot_address).await?;

    // Convert BTC amount to satoshis if provided
    let deposit_amount = convert_btc_to_amount(params.amount)?;

    let fee_rate = params
        .fee_rate
        .map(FeeRate::from_sat_per_vb_unchecked)
        .unwrap_or_else(|| FeeRate::from_sat_per_vb_unchecked(10)); // Default 10 sat/vbyte

    let signed_tx = crate::bitcoin_utils::sign_recovery_tx(
        &keypair,
        &params.citrea_addr,
        &params.recovery_taproot_address.address,
        &params.outpoint,
        deposit_amount.unwrap_or(config.bridge_amount),
        &params.destination_addr,
        fee_rate,
        config,
    )?;

    Ok(signed_tx)
}

pub fn verify_recovery_tx(
    params: VerifyRecoveryTxParams,
    config: &BridgeCliConfig,
) -> Result<(Txid, BitcoinAddress, Amount), BridgeCliError> {
    validate_address_purpose(&params.recovery_taproot_address, Purpose::Deposit)?;

    let (txid, address, amount) = crate::bitcoin_utils::verify_recovery_tx(
        &params.recovery_tx,
        &params.citrea_address,
        &params.recovery_taproot_address.address,
        convert_btc_to_amount(params.amount)?,
        config,
    )?;

    Ok((txid, address, amount))
}
