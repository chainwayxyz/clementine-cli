use crate::errors::BridgeCliError;
use crate::structs::TaprootAddressWithPrefix;
use crate::{BitcoinAddress, CitreaAddress, get_clementine_home_dir};
use bitcoin::Network;
use bitcoin::address::NetworkUnchecked;
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Wrapper for a stored deposit address used as a map key.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct StoredDepositAddress(pub String);

/// A single deposit address entry stored in the JSON file.
///
/// Contains the deposit address and associated Citrea address as strings.
/// Multiple entries can exist for each recovery taproot address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDepositEntry {
    pub recovery_taproot_address: String,
    pub citrea_address: String,
}

/// Internal details for a specific deposit address, keyed by deposit address.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredDepositDetails {
    pub network: Network,
    pub entry: StoredDepositEntry,
    pub created_at: u64,
}

/// Public, deposit-address-centric view of a stored deposit.
#[derive(Debug, Clone)]
pub struct DepositAddressDetails {
    pub deposit_address: String,
    pub recovery_taproot_address: String,
    pub citrea_address: String,
    pub network: Network,
    pub created_at: u64,
}

type StoredDepositMap = HashMap<StoredDepositAddress, StoredDepositDetails>;

pub fn store_deposit_address(deposit_data: &DepositData) -> Result<(), BridgeCliError> {
    let storage_path = get_clementine_home_dir()?.join(DEPOSIT_ADDRESS_STORAGE_FILE);

    let mut map: StoredDepositMap = if storage_path.exists() {
        let contents = fs::read_to_string(&storage_path)?;
        if contents.trim().is_empty() {
            StoredDepositMap::new()
        } else {
            serde_json::from_str::<StoredDepositMap>(&contents)?
        }
    } else {
        StoredDepositMap::new()
    };

    let key = StoredDepositAddress(deposit_data.deposit_address.to_string());

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_else(|e| {
            tracing::warn!("System time error, using 0 as timestamp: {}", e);
            0
        });

    let entry = StoredDepositEntry {
        recovery_taproot_address: deposit_data.recovery_taproot_address.address_with_prefix(),
        citrea_address: deposit_data.citrea_address.to_string(),
    };

    map.entry(key).or_insert_with(|| StoredDepositDetails {
        network: deposit_data.network,
        entry: entry.clone(),
        created_at: now,
    });

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

/// List all stored deposits, keyed by deposit address.
pub fn get_all_deposit_address_details() -> Result<Vec<DepositAddressDetails>, BridgeCliError> {
    let storage_path = get_clementine_home_dir()?.join(DEPOSIT_ADDRESS_STORAGE_FILE);

    if !storage_path.exists() {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(&storage_path)?;
    if contents.trim().is_empty() {
        return Ok(Vec::new());
    }

    let map: StoredDepositMap = serde_json::from_str::<StoredDepositMap>(&contents)?;

    let mut records: Vec<DepositAddressDetails> = map
        .into_iter()
        .map(
            |(StoredDepositAddress(deposit_address), details)| DepositAddressDetails {
                deposit_address,
                recovery_taproot_address: details.entry.recovery_taproot_address,
                citrea_address: details.entry.citrea_address,
                network: details.network,
                created_at: details.created_at,
            },
        )
        .collect();

    // Sort deterministically by creation time (and then by deposit address for tie-breaker)
    records.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.deposit_address.cmp(&b.deposit_address))
    });

    Ok(records)
}

/// Retrieves full deposit details for a specific deposit address.
///
/// The deposit address is treated as a string key; the function does not
/// perform any additional validation beyond lookup.
///
/// Returns `Ok(Some(..))` if the deposit address exists in storage, or
/// `Ok(None)` if it is not found.
pub fn get_deposit_address_details_for_deposit_address(
    deposit_address: &str,
) -> Result<Option<DepositAddressDetails>, BridgeCliError> {
    let storage_path = get_clementine_home_dir()?.join(DEPOSIT_ADDRESS_STORAGE_FILE);

    if !storage_path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&storage_path)?;

    if contents.trim().is_empty() {
        return Ok(None);
    }

    let map: StoredDepositMap = serde_json::from_str::<StoredDepositMap>(&contents)?;

    let key = StoredDepositAddress(deposit_address.to_string());

    if let Some(details) = map.get(&key) {
        Ok(Some(DepositAddressDetails {
            deposit_address: deposit_address.to_string(),
            recovery_taproot_address: details.entry.recovery_taproot_address.clone(),
            citrea_address: details.entry.citrea_address.clone(),
            network: details.network,
            created_at: details.created_at,
        }))
    } else {
        Ok(None)
    }
}
