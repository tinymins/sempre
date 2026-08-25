mod error;
mod install;
mod inventory;
mod lifecycle;
mod process;

use sempre_artifact::{Downloader, GithubClient};
use sempre_core::{Registry, Target, built_in_registry};
use sempre_state::{Document, Store};

pub use error::ManagerError;
pub use install::InstallResult;
pub use inventory::{CoreInventory, InstalledCore};
pub use lifecycle::CoreChange;
pub use process::{ProcessRunner, ValidationRunner, VersionRunner};

const USER_AGENT: &str = concat!("Sempre/", env!("CARGO_PKG_VERSION"));

pub struct Manager<R = ProcessRunner> {
    store: Store,
    registry: Registry,
    releases: GithubClient,
    downloader: Downloader,
    target: Target,
    runner: R,
}

impl Manager<ProcessRunner> {
    pub fn new(store: Store) -> Result<Self, ManagerError> {
        Self::with_runner(store, ProcessRunner)
    }
}

impl<R: VersionRunner> Manager<R> {
    pub fn with_runner(store: Store, runner: R) -> Result<Self, ManagerError> {
        store.initialize()?;
        Ok(Self {
            store,
            registry: built_in_registry(),
            releases: GithubClient::new(USER_AGENT)?,
            downloader: Downloader::new(USER_AGENT)?,
            target: Target::current(),
            runner,
        })
    }

    pub fn state(&self) -> Result<Document, ManagerError> {
        Ok(self.store.read()?)
    }

    pub fn store(&self) -> &Store {
        &self.store
    }
}
