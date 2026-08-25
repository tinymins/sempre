mod application_uninstall;
mod auto_config;
mod bundle;
mod component_deploy;
mod config;
mod context;
mod custom_node;
mod direct;
mod error;
mod gateway;
mod install;
mod inventory;
mod lifecycle;
mod process;
mod runtime;
mod scheduler;
mod service_deploy;
mod subscription;
mod subscription_mutation;
mod subscription_tools;
mod supervisor;
mod tunnel;
mod update;

use sempre_artifact::{Downloader, GithubClient};
use sempre_core::{Registry, Target, built_in_registry};
use sempre_state::{Document, Store};
use sempre_subscription::{Fetcher, RemoteClient, SubscriptionStore};
use sempre_transparent::Controller as TransparentController;
use sempre_tunnel::Controller as TunnelController;
use std::sync::Arc;
use tokio::sync::Notify;

pub use application_uninstall::{ApplicationUninstall, uninstall_application};
pub use auto_config::{
    AutoConfigApplyResult, AutoConfigCandidate, AutoConfigCheck, AutoConfigReport,
};
pub use config::{CurrentConfig, MAX_CONFIG_SIZE};
pub use context::{ConfigurationContext, ConfigurationTarget, RunningCore};
pub use error::ManagerError;
pub use install::InstallResult;
pub use inventory::{CoreInventory, InstalledCore};
pub use lifecycle::CoreChange;
pub use process::{ProcessRunner, ValidationRunner, VersionRunner};
pub use runtime::{RuntimeActionAvailability, RuntimeActions, RuntimeDeployment, RuntimeStatus};
pub use sempre_bundle::DeployComponent;
pub use sempre_bundle::Export as BundleExport;
pub use service_deploy::uninstall_system_service;
pub use subscription::SubscriptionRender;
pub use subscription_tools::{ProfileDebugResult, ProfileDebugSource, SourceTestResult};

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
    gateway: Arc<sempre_gateway::Controller>,
    runtime_reload: Arc<Notify>,
    subscription_schedule_changed: Arc<Notify>,
    tunnels: Arc<TunnelController>,
    transparent: Arc<TransparentController>,
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
        let gateway = Arc::new(sempre_gateway::Controller::new(store.layout())?);
        let tunnels = Arc::new(TunnelController::new(store.layout().clone())?);
        let transparent = Arc::new(TransparentController::new(store.layout()));
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
            gateway,
            runtime_reload: Arc::new(Notify::new()),
            subscription_schedule_changed: Arc::new(Notify::new()),
            tunnels,
            transparent,
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

    pub fn request_runtime_reload(&self) {
        self.runtime_reload.notify_one();
    }

    pub async fn wait_runtime_reload(&self) {
        self.runtime_reload.notified().await;
    }

    fn notify_subscription_schedule_changed(&self) {
        self.subscription_schedule_changed.notify_one();
    }

    async fn wait_subscription_schedule_changed(&self) {
        self.subscription_schedule_changed.notified().await;
    }
}
