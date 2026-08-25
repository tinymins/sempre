mod config;
mod context;
mod error;
mod install;
mod inventory;
mod lifecycle;
mod process;
mod subscription;

use sempre_artifact::{Downloader, GithubClient};
use sempre_core::{Registry, Target, built_in_registry};
use sempre_state::{Document, Store};
use sempre_subscription::{Fetcher, RemoteClient, SubscriptionStore};

pub use config::{CurrentConfig, MAX_CONFIG_SIZE};
pub use context::{ConfigurationContext, ConfigurationTarget, RunningCore};
pub use error::ManagerError;
pub use install::InstallResult;
pub use inventory::{CoreInventory, InstalledCore};
pub use lifecycle::CoreChange;
pub use process::{ProcessRunner, ValidationRunner, VersionRunner};
pub use subscription::SubscriptionRender;

const USER_AGENT: &str = concat!("Sempre/", env!("CARGO_PKG_VERSION"));

pub struct Manager<R = ProcessRunner> {
    store: Store,
    registry: Registry,
    releases: GithubClient,
    downloader: Downloader,
    target: Target,
    runner: R,
    subscriptions: SubscriptionStore,
    fetcher: Fetcher,
    remote: RemoteClient,
}

impl Manager<ProcessRunner> {
    pub fn new(store: Store) -> Result<Self, ManagerError> {
        Self::with_runner(store, ProcessRunner)
    }
}

impl<R: VersionRunner> Manager<R> {
    pub fn with_runner(store: Store, runner: R) -> Result<Self, ManagerError> {
        store.initialize()?;
        let subscriptions = SubscriptionStore::new(store.layout().clone());
        subscriptions.initialize()?;
        let fetcher = Fetcher::new(subscriptions.clone())?;
        let remote = RemoteClient::new()?;
        Ok(Self {
            store,
            registry: built_in_registry(),
            releases: GithubClient::new(USER_AGENT)?,
            downloader: Downloader::new(USER_AGENT)?,
            target: Target::current(),
            runner,
            subscriptions,
            fetcher,
            remote,
        })
    }

    pub fn state(&self) -> Result<Document, ManagerError> {
        Ok(self.store.read()?)
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn subscriptions(&self) -> &SubscriptionStore {
        &self.subscriptions
    }
}
