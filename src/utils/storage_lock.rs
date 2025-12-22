use std::{
    fs::{self, File, OpenOptions},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use crate::errors::BridgeCliError;

pub(crate) struct AtomicFileStorage<T> {
    storage_path: std::path::PathBuf,
    lock_file: File,
    _marker: PhantomData<T>,
}

struct LockGuard<'a> {
    file: &'a File,
}

impl Drop for LockGuard<'_> {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

// Cross-process shared/exclusive file lock.
// Note: platform-dependent semantics — may be advisory or mandatory, and
// may or may not block non-lockholders’ read/write operations.
impl<T> AtomicFileStorage<T> {
    pub fn new(
        storage_path: std::path::PathBuf,
        lock_path: std::path::PathBuf,
    ) -> Result<Self, BridgeCliError> {
        let lock_file = open_lock_file(&lock_path)?;
        Ok(Self {
            storage_path,
            lock_file,
            _marker: PhantomData,
        })
    }

    pub fn read_shared(&self) -> Result<T, BridgeCliError>
    where
        T: serde::de::DeserializeOwned,
    {
        let _guard = self.shared()?;

        if !check_file_exists(&self.storage_path) {
            return Err(BridgeCliError::FileNotFound(
                self.storage_path.to_string_lossy().to_string(),
            ));
        }

        let data = fs::read_to_string(&self.storage_path)?;
        let parsed: T = serde_json::from_str(&data)?;

        Ok(parsed)
    }

    pub fn insert_exclusive<F>(&self, modify_fn: F) -> Result<(), BridgeCliError>
    where
        T: serde::de::DeserializeOwned + serde::Serialize,
        F: FnOnce(&mut T),
    {
        let _guard = self.exclusive()?;

        let mut current_data = if check_file_exists(&self.storage_path) {
            let existing_data = fs::read_to_string(&self.storage_path)?;
            serde_json::from_str(&existing_data)?
        } else {
            serde_json::from_str("{}")? // Assuming T can be deserialized from an empty object
        };

        modify_fn(&mut current_data);

        self.write(&current_data)?;

        Ok(())
    }

    fn write(&self, data: &T) -> Result<(), BridgeCliError>
    where
        T: serde::Serialize,
    {
        let tmp_path = tmp_storage_path(&self.storage_path);

        if check_file_exists(&tmp_path) {
            return Err(BridgeCliError::Eyre(eyre::eyre!(
                "Refusing to proceed: temporary file '{}' already exists. \
                 This usually means a previous write was interrupted. \
                 Please reach out to support.",
                tmp_path.display()
            )));
        }

        let json_data = serde_json::to_string_pretty(data).map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!(
                "Failed to serialize storage data to JSON: {}",
                e
            ))
        })?;

        fs::write(&tmp_path, json_data).map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!(
                "Failed to write temporary storage file '{}': {}",
                tmp_path.display(),
                e
            ))
        })?;

        replace_storage_file(&tmp_path, &self.storage_path)?;

        Ok(())
    }

    fn shared(&self) -> Result<LockGuard<'_>, BridgeCliError> {
        self.lock_file.lock_shared().map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!(
                "Failed to acquire shared lock on storage file: {}",
                e
            ))
        })?;
        Ok(LockGuard {
            file: &self.lock_file,
        })
    }

    fn exclusive(&self) -> Result<LockGuard<'_>, BridgeCliError> {
        self.lock_file.lock().map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!(
                "Failed to acquire exclusive lock on storage file: {}",
                e
            ))
        })?;
        Ok(LockGuard {
            file: &self.lock_file,
        })
    }
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

fn tmp_storage_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        "{}.tmp",
        path.file_name()
            .expect("Storage path has a file name")
            .to_string_lossy()
    ))
}

fn bak_storage_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        "{}.bak",
        path.file_name()
            .expect("Storage path has a file name")
            .to_string_lossy()
    ))
}

fn replace_storage_file(tmp_path: &Path, final_path: &Path) -> Result<(), BridgeCliError> {
    let bak_path = bak_storage_path(final_path);

    if bak_path.exists() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Refusing to proceed: backup file '{}' already exists. \
             This usually means a previous write was interrupted. \
             Please reach out to support.",
            bak_path.display()
        )));
    }

    if !final_path.exists() {
        fs::rename(tmp_path, final_path).map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!(
                "Failed to move temporary storage file '{}' into place '{}': {}",
                tmp_path.display(),
                final_path.display(),
                e
            ))
        })?;
        return Ok(());
    }

    fs::copy(final_path, &bak_path).map_err(|e| {
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to create backup by copying '{}' -> '{}': {}",
            final_path.display(),
            bak_path.display(),
            e
        ))
    })?;

    if fs::rename(tmp_path, final_path).is_ok() {
        let _ = fs::remove_file(&bak_path); // best-effort
        return Ok(());
    }

    // Windows path: remove final then rename.
    // If remove fails, keep bak + tmp for recovery.
    if let Err(e) = fs::remove_file(final_path) {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Failed to remove existing storage file '{}': {}. \
             Backup is at '{}', temp is at '{}'.",
            final_path.display(),
            e,
            bak_path.display(),
            tmp_path.display(),
        )));
    }

    match fs::rename(tmp_path, final_path) {
        Ok(_) => {
            let _ = fs::remove_file(&bak_path); // best-effort
            Ok(())
        }
        Err(e) => {
            // Try to restore backup so final isn't left missing.
            let _ = fs::copy(&bak_path, final_path);
            Err(BridgeCliError::Eyre(eyre::eyre!(
                "Failed to replace storage file '{}': {}. \
                 Backup is at '{}', temp is at '{}'.",
                final_path.display(),
                e,
                bak_path.display(),
                tmp_path.display(),
            )))
        }
    }
}

fn check_file_exists(path: &Path) -> bool {
    path.exists()
}
