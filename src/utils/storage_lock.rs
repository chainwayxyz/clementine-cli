use std::{fs::{self, File, OpenOptions}, marker::PhantomData, path::{Path, PathBuf}};

use crate::errors::BridgeCliError;

struct AtomicFileStorage<T> {
    storage_path: std::path::PathBuf,
    lock_file: File,
    _marker: PhantomData<T>,
}



// Cross-process shared/exclusive file lock.
// Note: platform-dependent semantics — may be advisory or mandatory, and
// may or may not block non-lockholders’ read/write operations.
impl<T> AtomicFileStorage<T> {
    fn new(storage_path: std::path::PathBuf, lock_path: std::path::PathBuf) -> Result<Self, BridgeCliError> {
        let lock_file = open_lock_file(&lock_path)?;
        Ok(Self {
            storage_path,
            lock_file,
            _marker: PhantomData,
        })
    }

    fn read_shared(&self) -> Result<T, BridgeCliError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.shared()?;
        let data = fs::read_to_string(&self.storage_path)?;
        let parsed: T = serde_json::from_str(&data)?;
        Ok(parsed)
    }

    fn write_exclusive(&self, data: &T) -> Result<(), BridgeCliError>
    where
        T: serde::Serialize,
    {
        self.exclusive()?;
        let tmp_path = tmp_storage_path(&self.storage_path);
        let json_data = serde_json::to_string_pretty(data)?;
        fs::write(&tmp_path, json_data)?;
        fs::rename(&tmp_path, &self.storage_path)?;
        Ok(())
    }

    fn shared(&self) -> Result<(), BridgeCliError> {
        self.lock_file.lock_shared()?;
        Ok(())
    }

    fn exclusive(&self) -> Result<(), BridgeCliError> {
        self.lock_file.lock()?;
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