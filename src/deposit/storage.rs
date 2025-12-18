use crate::errors::BridgeCliError;
use crate::structs::TaprootAddressWithPrefix;
use crate::{BitcoinAddress, CitreaAddress, get_clementine_home_dir};
use bitcoin::Network;
use bitcoin::address::NetworkUnchecked;
use bitcoin::secp256k1::XOnlyPublicKey;
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Data needed to store a deposit address.
#[derive(Debug, Clone)]
pub struct DepositData {
    pub deposit_address: BitcoinAddress,
    pub recovery_taproot_address: TaprootAddressWithPrefix<NetworkUnchecked>,
    pub aggregated_public_key: XOnlyPublicKey,
    pub citrea_address: CitreaAddress,
    pub user_takes_after: u64,
    pub network: Network,
}

const DEPOSIT_ADDRESS_STORAGE_FILE: &str = "deposit_addresses.json";

// Use a separate lock file so we can coordinate readers/writers while
// atomically replacing deposit_addresses.json (esp. on Windows, where
// replacing/renaming an open file can fail).
const DEPOSIT_ADDRESS_LOCK_FILE: &str = "deposit_addresses.lock";

/// Map key for a stored deposit address.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct StoredDepositAddress(pub String);

/// Stored metadata for a deposit address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDepositEntry {
    pub aggregated_public_key: String,
    pub recovery_taproot_address: String,
    pub citrea_address: String,
    pub user_takes_after: u64,
}

/// Internal record for a deposit address.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredDepositDetails {
    pub network: Network,
    pub entry: StoredDepositEntry,
    pub created_at: u64,
}

/// Public view of a stored deposit address.
#[derive(Debug, Clone)]
pub struct DepositAddressDetails {
    pub deposit_address: String,
    pub aggregated_public_key: String,
    pub recovery_taproot_address: String,
    pub citrea_address: String,
    pub user_takes_after: u64,
    pub network: Network,
    pub created_at: u64,
}

type StoredDepositMap = HashMap<StoredDepositAddress, StoredDepositDetails>;

struct FileLock {
    file: File,
}

// Cross-process shared/exclusive file lock.
// Note: platform-dependent semantics — may be advisory or mandatory, and
// may or may not block non-lockholders’ read/write operations.
impl FileLock {
    fn shared(path: &Path) -> Result<Self, BridgeCliError> {
        let file = open_lock_file(path)?;
        file.lock_shared()?;
        Ok(Self { file })
    }

    fn exclusive(path: &Path) -> Result<Self, BridgeCliError> {
        let file = open_lock_file(path)?;
        file.lock()?;
        Ok(Self { file })
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Store a deposit address and its metadata.
///
/// If the address already exists, the existing record is kept.
/// Returns an error if the storage file cannot be read or written.
pub fn store_deposit_address(deposit_data: &DepositData) -> Result<(), BridgeCliError> {
    let storage_path = storage_path()?;
    let lock_path = lock_path()?;

    // Exclusive lock to coordinate writers/readers across processes.
    let _lock = FileLock::exclusive(&lock_path)?;

    let mut map = read_storage_map(&storage_path)?.unwrap_or_default();

    if let Some(parent) = storage_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let key = StoredDepositAddress(deposit_data.deposit_address.to_string());

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_else(|e| {
            tracing::warn!("System time error, using 0 as timestamp: {}", e);
            0
        });

    let entry = StoredDepositEntry {
        aggregated_public_key: deposit_data.aggregated_public_key.to_string(),
        recovery_taproot_address: deposit_data.recovery_taproot_address.address_with_prefix(),
        citrea_address: deposit_data.citrea_address.to_string(),
        user_takes_after: deposit_data.user_takes_after,
    };

    map.entry(key).or_insert_with(|| StoredDepositDetails {
        network: deposit_data.network,
        entry: entry.clone(),
        created_at: now,
    });

    let tmp_path = tmp_storage_path(&storage_path);

    let json = serde_json::to_string_pretty(&map)?;

    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
    }

    replace_storage_file(&tmp_path, &storage_path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&storage_path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&storage_path, perms)?;
    }

    Ok(())
}

/// List all stored deposits.
///
/// Returns an empty vector if no deposits have been stored.
pub fn get_all_deposit_address_details() -> Result<Vec<DepositAddressDetails>, BridgeCliError> {
    let storage_path = storage_path()?;
    let lock_path = lock_path()?;

    let _lock = FileLock::shared(&lock_path)?;

    let Some(map) = read_storage_map(&storage_path)? else {
        return Ok(Vec::new());
    };

    let mut records: Vec<DepositAddressDetails> = map
        .into_iter()
        .map(
            |(StoredDepositAddress(deposit_address), details)| DepositAddressDetails {
                deposit_address,
                aggregated_public_key: details.entry.aggregated_public_key,
                recovery_taproot_address: details.entry.recovery_taproot_address,
                citrea_address: details.entry.citrea_address,
                user_takes_after: details.entry.user_takes_after,
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
    let storage_path = storage_path()?;
    let lock_path = lock_path()?;

    let _lock = FileLock::shared(&lock_path)?;

    let Some(map) = read_storage_map(&storage_path)? else {
        return Ok(None);
    };

    let key = StoredDepositAddress(deposit_address.to_string());

    if let Some(details) = map.get(&key) {
        Ok(Some(DepositAddressDetails {
            deposit_address: deposit_address.to_string(),
            aggregated_public_key: details.entry.aggregated_public_key.clone(),
            recovery_taproot_address: details.entry.recovery_taproot_address.clone(),
            citrea_address: details.entry.citrea_address.clone(),
            user_takes_after: details.entry.user_takes_after,
            network: details.network,
            created_at: details.created_at,
        }))
    } else {
        Ok(None)
    }
}

fn storage_path() -> Result<PathBuf, BridgeCliError> {
    Ok(get_clementine_home_dir()?.join(DEPOSIT_ADDRESS_STORAGE_FILE))
}

fn lock_path() -> Result<PathBuf, BridgeCliError> {
    Ok(get_clementine_home_dir()?.join(DEPOSIT_ADDRESS_LOCK_FILE))
}

fn open_lock_file(path: &Path) -> Result<File, BridgeCliError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?)
}

fn read_storage_map(path: &Path) -> Result<Option<StoredDepositMap>, BridgeCliError> {
    if !path.exists() {
        return Ok(None);
    }

    let mut contents = String::new();
    {
        let mut file = OpenOptions::new().read(true).open(path)?;
        file.read_to_string(&mut contents)?;
    }

    if contents.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(serde_json::from_str::<StoredDepositMap>(&contents)?))
}

fn tmp_storage_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        "{}.tmp",
        path.file_name()
            .expect("Storage path has a file name")
            .to_string_lossy()
    ))
}

fn replace_storage_file(tmp_path: &Path, final_path: &Path) -> Result<(), BridgeCliError> {
    match fs::rename(tmp_path, final_path) {
        Ok(_) => return Ok(()),
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(final_path)?;
        }
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied => {
            // On some platforms (notably Windows), an existing file may block overwrite.
            let _ = fs::remove_file(final_path);
        }
        Err(err) => {
            let _ = fs::remove_file(tmp_path);
            return Err(err.into());
        }
    }

    fs::rename(tmp_path, final_path).map_err(|e| {
        let _ = fs::remove_file(tmp_path);
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to replace deposit address storage file: {}",
            e
        ))
    })?;

    Ok(())
}
