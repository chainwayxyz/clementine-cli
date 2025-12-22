use std::{
    fs::{self, File, OpenOptions},
    io,
    marker::PhantomData,
    path::{Path, PathBuf},
};

use crate::errors::BridgeCliError;

pub(crate) struct AtomicFileStorage<T> {
    storage_path: std::path::PathBuf,
    lock_file: File,
    _marker: PhantomData<T>,
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
        self.shared()?;

        if !check_file_exists(&self.storage_path) {
            return Err(BridgeCliError::FileNotFound(
                self.storage_path.to_string_lossy().to_string(),
            ));
        }

        let data = fs::read_to_string(&self.storage_path)?;
        let parsed: T = serde_json::from_str(&data)?;

        self.unlock()?;
        Ok(parsed)
    }

    pub fn insert_exclusive<F>(&self, modify_fn: F) -> Result<(), BridgeCliError>
    where
        T: serde::de::DeserializeOwned + serde::Serialize,
        F: FnOnce(&mut T),
    {
        self.exclusive()?;

        let mut current_data = if check_file_exists(&self.storage_path) {
            let existing_data = fs::read_to_string(&self.storage_path)?;
            serde_json::from_str(&existing_data)?
        } else {
            serde_json::from_str("{}")? // Assuming T can be deserialized from an empty object
        };

        modify_fn(&mut current_data);

        self.write(&current_data)?;

        self.unlock()?;
        Ok(())
    }

    fn write(&self, data: &T) -> Result<(), BridgeCliError>
    where
        T: serde::Serialize,
    {
        let tmp_path = tmp_storage_path(&self.storage_path);

        if check_file_exists(&tmp_path) {
            return Err(BridgeCliError::Eyre(eyre::eyre!(
                "Temporary storage file '{}' already exists. Aborting write to prevent data loss. Please reach out to support.",
                tmp_path.display()
            )));
        };

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

    fn shared(&self) -> Result<(), BridgeCliError> {
        self.lock_file.lock_shared().map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!(
                "Failed to acquire shared lock on storage file: {}",
                e
            ))
        })?;
        Ok(())
    }

    fn exclusive(&self) -> Result<(), BridgeCliError> {
        self.lock_file.lock().map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!(
                "Failed to acquire exclusive lock on storage file: {}",
                e
            ))
        })?;
        Ok(())
    }

    fn unlock(&self) -> Result<(), BridgeCliError> {
        self.lock_file.unlock().map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!("Failed to release lock on storage file: {}", e))
        })?;
        Ok(())
    }
}

impl<T> Drop for AtomicFileStorage<T> {
    fn drop(&mut self) {
        let _ = self.lock_file.unlock();
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
            return Err(BridgeCliError::Eyre(eyre::eyre!(
                "Failed to replace deposit address storage file: {}",
                err
            )));
        }
    }

    fs::rename(tmp_path, final_path).map_err(|e| {
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to replace deposit address storage file: {}",
            e
        ))
    })?;

    fs::remove_file(tmp_path).map_err(|e| {
        BridgeCliError::Eyre(eyre::eyre!(
            "Failed to remove temporary storage file '{}': {}",
            tmp_path.display(),
            e
        ))
    })?;

    Ok(())
}

fn check_file_exists(path: &Path) -> bool {
    path.exists()
}
