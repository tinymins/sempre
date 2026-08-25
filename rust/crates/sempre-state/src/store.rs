use std::{fs, fs::File, fs::OpenOptions, io, path::Path};

use chrono::Utc;
use fs2::FileExt as _;
use thiserror::Error;

use crate::{Document, Layout, LayoutError, StateValidationError, write_atomic};

#[derive(Debug, Error)]
pub enum StateError {
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error("open lock {path}: {source}")]
    OpenLock { path: Box<Path>, source: io::Error },
    #[error("lock state: {0}")]
    Lock(#[source] io::Error),
    #[error("read state: {0}")]
    Read(#[source] io::Error),
    #[error("decode state: {0}")]
    Decode(#[source] serde_json::Error),
    #[error(transparent)]
    Validate(#[from] StateValidationError),
    #[error("encode state: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("write state: {0}")]
    Write(#[source] io::Error),
    #[error("another Sempre instance is already running")]
    AlreadyRunning,
}

#[derive(Clone, Debug)]
pub struct Store {
    layout: Layout,
}

pub struct Lease {
    file: File,
}

impl Store {
    pub fn new(layout: Layout) -> Self {
        Self { layout }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn initialize(&self) -> Result<Document, StateError> {
        self.layout.ensure()?;
        self.with_lock(|| {
            if self.layout.state.exists() {
                self.read_unlocked()
            } else {
                let document = Document::default();
                self.write_unlocked(&document)?;
                Ok(document)
            }
        })
    }

    pub fn read(&self) -> Result<Document, StateError> {
        self.with_lock(|| self.read_unlocked())
    }

    pub fn update<F>(&self, update: F) -> Result<Document, StateError>
    where
        F: FnOnce(&mut Document) -> Result<(), StateValidationError>,
    {
        self.with_lock(|| {
            let mut document = self.read_unlocked()?;
            update(&mut document)?;
            document.updated_at = Utc::now();
            document.validate()?;
            self.write_unlocked(&document)?;
            Ok(document)
        })
    }

    pub fn acquire_instance(&self) -> Result<Lease, StateError> {
        self.layout.ensure_instance_lock_directory()?;
        let file = open_lock(&self.layout.instance_lock)?;
        file.try_lock_exclusive()
            .map_err(|_| StateError::AlreadyRunning)?;
        Ok(Lease { file })
    }

    pub fn acquire_operation(&self) -> Result<Lease, StateError> {
        self.layout.ensure()?;
        let file = open_lock(&self.layout.operation_lock)?;
        file.lock_exclusive().map_err(StateError::Lock)?;
        Ok(Lease { file })
    }

    fn with_lock<T>(
        &self,
        action: impl FnOnce() -> Result<T, StateError>,
    ) -> Result<T, StateError> {
        let file = open_lock(&self.layout.state_lock)?;
        file.lock_exclusive().map_err(StateError::Lock)?;
        let result = action();
        let _ = file.unlock();
        result
    }

    fn read_unlocked(&self) -> Result<Document, StateError> {
        let data = fs::read(&self.layout.state).map_err(StateError::Read)?;
        let document: Document = serde_json::from_slice(&data).map_err(StateError::Decode)?;
        document.validate()?;
        Ok(document)
    }

    fn write_unlocked(&self, document: &Document) -> Result<(), StateError> {
        document.validate()?;
        let mut data = serde_json::to_vec_pretty(document).map_err(StateError::Encode)?;
        data.push(b'\n');
        write_atomic(&self.layout.state, &data, 0o600).map_err(StateError::Write)
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn open_lock(path: &Path) -> Result<File, StateError> {
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|source| StateError::OpenLock {
            path: path.into(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DesiredState;

    #[test]
    fn initializes_and_updates_state_atomically() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let store = Store::new(Layout::at(temporary.path()));
        let initial = store.initialize().expect("initialize state");
        assert_eq!(initial.desired_state, DesiredState::Running);
        let updated = store
            .update(|document| {
                document.desired_state = DesiredState::Stopped;
                Ok(())
            })
            .expect("update state");
        assert_eq!(updated.desired_state, DesiredState::Stopped);
        assert_eq!(store.read().expect("read state"), updated);
    }

    #[test]
    fn rejects_a_second_instance_lease() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let store = Store::new(Layout::at(temporary.path()));
        store.initialize().expect("initialize state");
        let _lease = store.acquire_instance().expect("first lease");
        assert!(matches!(
            store.acquire_instance(),
            Err(StateError::AlreadyRunning)
        ));
    }

    #[test]
    fn operation_lease_uses_the_dedicated_lock() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let store = Store::new(Layout::at(temporary.path()));
        store.initialize().expect("initialize state");
        let _lease = store.acquire_operation().expect("operation lease");
        assert!(store.layout.operation_lock.exists());
    }

    #[test]
    fn refuses_an_unknown_schema() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let store = Store::new(Layout::at(temporary.path()));
        store.initialize().expect("initialize state");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&store.layout.state).expect("read fixture"))
                .expect("decode fixture");
        value["schema"] = 99.into();
        fs::write(
            &store.layout.state,
            serde_json::to_vec(&value).expect("encode fixture"),
        )
        .expect("write fixture");
        assert!(matches!(
            store.read(),
            Err(StateError::Validate(StateValidationError::Schema(99)))
        ));
    }
}
