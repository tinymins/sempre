mod application_uninstall;
mod auto_config;
mod bundle;
mod component_deploy;
mod config;
mod config_build;
mod context;
mod custom_node;
mod direct;
mod dns_capture;
mod dns_frontend;
mod dns_routing;
mod dns_runtime;
mod dns_settings;
mod error;
mod gateway;
mod install;
mod inventory;
mod lifecycle;
mod network_automation;
mod network_settings;
mod pending_changes;
mod private_access_status;
mod process;
mod restart_task;
mod rule_bootstrap;
mod rule_provider;
mod runtime;
mod runtime_ports;
mod scheduler;
mod selection_config;
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
pub use dns_routing::{DnsRoutingDomain, DnsRoutingRuleSet};
pub use dns_runtime::DnsFrontendStatus;
pub use dns_settings::DnsSettings;
pub use error::ManagerError;
pub use install::InstallResult;
pub use inventory::{CoreInventory, InstalledCore};
pub use lifecycle::CoreChange;
pub use network_automation::NetworkAutomationStatus;
pub use network_settings::{KnownNetwork, NetworkMode, NetworkSettings};
pub use pending_changes::RuntimePendingChange;
pub use private_access_status::{PrivateAccessConnectorStatus, PrivateAccessStatus};
pub use process::{ProcessRunner, ValidationRunner, VersionRunner};
pub use restart_task::{RestartLogEntry, RestartTask};
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
    restart_tasks: Arc<restart_task::RestartTasks>,
    subscription_schedule_changed: Arc<Notify>,
    tunnels: Arc<TunnelController>,
    transparent: Arc<TransparentController>,
    dns_frontend: Arc<dns_runtime::DnsFrontendRuntime>,
    dns_settings: Arc<dns_settings::DnsSettingsStore>,
    network_settings: Arc<network_settings::NetworkSettingsStore>,
}

impl Manager<ProcessRunner> {
    pub fn new(store: Store) -> Result<Self, ManagerError> {
        Self::with_runner(store, ProcessRunner)
    }
}

impl<R: VersionRunner> Manager<R> {
    pub fn with_runner(store: Store, runner: R) -> Result<Self, ManagerError> {
        let document = store.initialize()?;
        let subscriptions = SubscriptionStore::new(store.layout().clone());
        let catalog = subscriptions.initialize()?;
        let active_profile_exists = document
            .active_profile_id
            .as_ref()
            .is_some_and(|id| catalog.profiles.iter().any(|profile| profile.id == *id));
        if !active_profile_exists {
            let profile_id = catalog.profiles[0].id.clone();
            store.update(|document| {
                document.active_profile_id = Some(profile_id);
                Ok(())
            })?;
        }
        let document = store.read()?;
        let initial_profile = document
            .active_profile_id
            .as_deref()
            .and_then(|id| catalog.profiles.iter().find(|profile| profile.id == id))
            .unwrap_or(&catalog.profiles[0]);
        let dns_settings = Arc::new(dns_settings::DnsSettingsStore::open(
            store.layout().dns_settings.clone(),
            store.layout().dns_query_history.clone(),
            initial_profile,
        )?);
        let network_settings = Arc::new(network_settings::NetworkSettingsStore::open(
            store.layout().network_settings.clone(),
        )?);
        let fetcher = Fetcher::new(subscriptions.clone())?;
        let remote = RemoteClient::new()?;
        let gateway = Arc::new(sempre_gateway::Controller::new(store.layout())?);
        let tunnels = Arc::new(TunnelController::new(store.layout().clone())?);
        let transparent = Arc::new(TransparentController::new(store.layout()));
        let dns_frontend = dns_runtime::DnsFrontendRuntime::new(
            dns_settings.clone(),
            (store.layout().mode == sempre_state::Mode::System)
                .then(|| store.layout().resources.clone()),
        );
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
            restart_tasks: Arc::default(),
            subscription_schedule_changed: Arc::new(Notify::new()),
            tunnels,
            transparent,
            dns_frontend,
            dns_settings,
            network_settings,
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

    pub fn dns_settings(&self) -> DnsSettings {
        self.dns_settings.read()
    }

    pub fn dns_frontend_status(&self) -> DnsFrontendStatus {
        self.dns_frontend.status()
    }

    pub fn network_settings(&self) -> NetworkSettings {
        self.network_settings.read()
    }

    pub fn dns_queries(&self) -> Vec<sempre_dns::DnsQueryEvent> {
        self.dns_settings.queries()
    }

    pub fn clear_dns_queries(&self) -> Result<(), ManagerError> {
        self.dns_settings.clear_queries()
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
