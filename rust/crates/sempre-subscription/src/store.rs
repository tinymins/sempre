use std::{fs, fs::File, fs::OpenOptions, path::Path};

use chrono::Utc;
use fs2::FileExt as _;
use sha2::{Digest, Sha256};

use crate::{Catalog, MAX_SOURCE_SIZE, SubscriptionError, validate};

#[derive(Clone, Debug)]
pub struct SubscriptionStore {
    layout: sempre_state::Layout,
}

impl SubscriptionStore {
    pub fn new(layout: sempre_state::Layout) -> Self {
        Self { layout }
    }

    pub fn initialize(&self) -> Result<Catalog, SubscriptionError> {
        self.with_lock(|| {
            if self.layout.subscription_catalog.exists() {
                self.read_unlocked()
            } else {
                let catalog = Catalog::default();
                self.write_unlocked(&catalog)?;
                Ok(catalog)
            }
        })
    }

    pub fn read(&self) -> Result<Catalog, SubscriptionError> {
        self.with_lock(|| self.read_unlocked())
    }

    pub fn update(
        &self,
        change: impl FnOnce(&mut Catalog) -> Result<(), SubscriptionError>,
    ) -> Result<Catalog, SubscriptionError> {
        self.with_lock(|| {
            let mut catalog = self.read_unlocked()?;
            change(&mut catalog)?;
            catalog.updated_at = Utc::now();
            validate::catalog(&catalog)?;
            self.write_unlocked(&catalog)?;
            Ok(catalog)
        })
    }

    pub fn save_blob(&self, content: &[u8]) -> Result<String, SubscriptionError> {
        if content.len() > MAX_SOURCE_SIZE {
            return Err(SubscriptionError::SourceTooLarge {
                limit: MAX_SOURCE_SIZE,
            });
        }
        let hash = format!("{:x}", Sha256::digest(content));
        let path = self.layout.subscription_blobs.join(&hash);
        if path.exists() {
            let existing = self.read_blob(&hash)?;
            if existing == content {
                return Ok(hash);
            }
            return Err(SubscriptionError::SnapshotIntegrity { hash });
        }
        sempre_state::write_atomic(&path, content, 0o600)
            .map_err(SubscriptionError::WriteSnapshot)?;
        Ok(hash)
    }

    pub fn read_blob(&self, hash: &str) -> Result<Vec<u8>, SubscriptionError> {
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(SubscriptionError::InvalidHash(hash.into()));
        }
        let hash = hash.to_ascii_lowercase();
        let content = fs::read(self.layout.subscription_blobs.join(&hash))
            .map_err(SubscriptionError::ReadSnapshot)?;
        if format!("{:x}", Sha256::digest(&content)) != hash {
            return Err(SubscriptionError::SnapshotIntegrity { hash });
        }
        Ok(content)
    }

    pub fn clear_cache(&self) -> Result<(), SubscriptionError> {
        let entries = match fs::read_dir(&self.layout.subscription_cache) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(SubscriptionError::ReadCache(error)),
        };
        for entry in entries {
            let entry = entry.map_err(SubscriptionError::ReadCache)?;
            if entry
                .file_type()
                .map_err(SubscriptionError::ReadCache)?
                .is_file()
            {
                fs::remove_file(entry.path()).map_err(SubscriptionError::WriteCache)?;
            }
        }
        Ok(())
    }

    pub(crate) fn cache_path(&self, key: &str) -> std::path::PathBuf {
        let hash = format!("{:x}", Sha256::digest(key.as_bytes()));
        self.layout.subscription_cache.join(format!("{hash}.json"))
    }

    fn with_lock<T>(
        &self,
        action: impl FnOnce() -> Result<T, SubscriptionError>,
    ) -> Result<T, SubscriptionError> {
        fs::create_dir_all(&self.layout.subscriptions).map_err(SubscriptionError::Write)?;
        fs::create_dir_all(&self.layout.subscription_blobs)
            .map_err(SubscriptionError::WriteSnapshot)?;
        let file = open_lock(&self.layout.subscription_lock)?;
        file.lock_exclusive().map_err(SubscriptionError::Lock)?;
        let result = action();
        let _ = file.unlock();
        result
    }

    fn read_unlocked(&self) -> Result<Catalog, SubscriptionError> {
        let data = fs::read(&self.layout.subscription_catalog).map_err(SubscriptionError::Read)?;
        let catalog = serde_json::from_slice(&data).map_err(SubscriptionError::Decode)?;
        validate::catalog(&catalog)?;
        Ok(catalog)
    }

    fn write_unlocked(&self, catalog: &Catalog) -> Result<(), SubscriptionError> {
        validate::catalog(catalog)?;
        let mut data = serde_json::to_vec_pretty(catalog).map_err(SubscriptionError::Encode)?;
        data.push(b'\n');
        sempre_state::write_atomic(&self.layout.subscription_catalog, &data, 0o600)
            .map_err(SubscriptionError::Write)
    }
}

fn open_lock(path: &Path) -> Result<File, SubscriptionError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(SubscriptionError::Write)?;
    }
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|source| SubscriptionError::OpenLock {
            path: path.into(),
            source,
        })
}

#[cfg(test)]
mod tests;
