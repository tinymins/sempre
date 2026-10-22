mod update;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use ipnet::IpNet;
use sempre_converter::DnsFrontendPolicy;
use sempre_core::CoreRef;
use sempre_dns::{DnsConfig, DnsRuntimePolicy, DnsService};
use sempre_state::{Deployment, Document};
use sempre_transparent::Plan as TransparentPlan;
use tokio::sync::Mutex;

use crate::{DnsFrontendStatus, Manager, ManagerError, ValidationRunner, VersionRunner};

pub(crate) struct DnsFrontendRuntime {
    running: Mutex<Option<RunningFrontend>>,
    status: RwLock<DnsFrontendStatus>,
    policy: Arc<dyn DnsRuntimePolicy>,
    resources: Option<PathBuf>,
    capture_error: crate::dns_capture::CaptureError,
}

struct RunningFrontend {
    plan: DnsFrontendPlan,
    service: DnsService,
    capture: Option<crate::dns_capture::Capture>,
}

#[derive(Clone)]
pub(crate) struct DnsFrontendPlan {
    pub(crate) deployment_hash: String,
    pub(crate) config: DnsConfig,
    fakeip_enabled: bool,
    fakeip_ranges: Vec<IpNet>,
    core_upstream: String,
    original_upstreams: Vec<String>,
}

impl DnsFrontendRuntime {
    pub(crate) fn new(policy: Arc<dyn DnsRuntimePolicy>, resources: Option<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            running: Mutex::new(None),
            status: RwLock::new(DnsFrontendStatus::default()),
            policy,
            resources,
            capture_error: Arc::default(),
        })
    }

    pub(crate) async fn prepare(
        &self,
        frontend: Option<&DnsFrontendPlan>,
    ) -> Result<(), ManagerError> {
        let Some(frontend) = frontend else {
            return Ok(());
        };
        self.start_if_missing(frontend).await
    }

    pub(crate) async fn activate_core(
        &self,
        frontend: Option<&DnsFrontendPlan>,
    ) -> Result<(), ManagerError> {
        let Some(frontend) = frontend else {
            return Ok(());
        };
        let result = self.promote(frontend).await;
        if let Err(error) = &result {
            self.record_failure(error);
        }
        result
    }

    pub(crate) async fn stop(&self) {
        if let Some(running) = self.running.lock().await.take() {
            if let Some(capture) = running.capture {
                capture.stop().await;
            }
            running.service.stop().await;
        }
        *self.status.write().expect("DNS frontend status") = DnsFrontendStatus::default();
        *self.capture_error.write().expect("capture status") = None;
    }

    fn configure(&self, plan: &DnsFrontendPlan) {
        let port_53 = self
            .status
            .read()
            .expect("DNS frontend status")
            .port_53
            .clone();
        *self.status.write().expect("DNS frontend status") = DnsFrontendStatus {
            enabled: true,
            running: true,
            fakeip_ranges: if plan.fakeip_enabled {
                plan.fakeip_ranges.clone()
            } else {
                Vec::new()
            },
            port_53,
            mode: if plan.fakeip_enabled {
                "fake-ip"
            } else {
                "real-ip"
            }
            .into(),
            core_upstream: plan.core_upstream.clone(),
            original_upstreams: plan.original_upstreams.clone(),
            direct_upstreams: plan.config.local_upstreams.clone(),
            domestic_domain_source: sempre_dns::DOMESTIC_DOMAIN_SOURCE.into(),
            domestic_domain_sha256: sempre_dns::DOMESTIC_DOMAIN_SHA256.into(),
            domestic_domain_count: sempre_dns::DOMESTIC_DOMAIN_COUNT,
            last_error: String::new(),
        };
    }

    pub(crate) fn status(&self) -> DnsFrontendStatus {
        let mut status = self.status.read().expect("DNS frontend status").clone();
        if let Some(error) = &*self.capture_error.read().expect("capture status") {
            status.running = false;
            status.last_error.clone_from(error);
        }
        status
    }

    pub(crate) fn record_failure(&self, error: &impl ToString) {
        let mut status = self.status.write().expect("DNS frontend status");
        status.last_error = error.to_string();
    }

    async fn start_if_missing(&self, plan: &DnsFrontendPlan) -> Result<(), ManagerError> {
        let mut running = self.running.lock().await;
        if let Some(current) = running.as_mut() {
            if self.capture_error.read().expect("capture status").is_none() {
                return Ok(());
            }
            current.capture = crate::dns_capture::Capture::start(
                self.resources.as_deref(),
                current.plan.config.listen_port,
                Arc::clone(&self.capture_error),
            )
            .await?;
            return Ok(());
        }
        let (service, port_53) =
            crate::dns_listener::start_dns_service(plan.config.clone(), Arc::clone(&self.policy))
                .await?;
        let capture = match crate::dns_capture::Capture::start(
            self.resources.as_deref(),
            plan.config.listen_port,
            Arc::clone(&self.capture_error),
        )
        .await
        {
            Ok(capture) => capture,
            Err(error) => {
                service.stop().await;
                return Err(error);
            }
        };
        *running = Some(RunningFrontend {
            plan: plan.clone(),
            service,
            capture,
        });
        self.configure(plan);
        self.status.write().expect("DNS frontend status").port_53 = port_53;
        Ok(())
    }

    async fn promote(&self, plan: &DnsFrontendPlan) -> Result<(), ManagerError> {
        self.start_if_missing(plan).await?;
        let mut running = self.running.lock().await;
        let current = running.as_mut().expect("started DNS frontend");
        if current.plan.deployment_hash != plan.deployment_hash
            || current.plan.config != plan.config
        {
            current.service.update(plan.config.clone())?;
            current.plan = plan.clone();
        }
        self.configure(plan);
        Ok(())
    }
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn dns_diagnostic_handle(&self) -> Option<sempre_dns::DnsDiagnosticHandle> {
        self.dns_frontend
            .running
            .lock()
            .await
            .as_ref()
            .map(|running| running.service.diagnostic_handle())
    }

    pub(crate) async fn prepare_dns_frontend_plan(
        &self,
        document: &Document,
        deployment: &Deployment,
        reference: &CoreRef,
    ) -> Result<Option<DnsFrontendPlan>, ManagerError> {
        let Some(profile_id) = document.active_profile_id.as_deref() else {
            return Ok(None);
        };
        let catalog = self.subscriptions.read()?;
        let profile = catalog
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| ManagerError::ProfileNotFound(profile_id.into()))?;
        let (target, _) = self.subscription_target_for(reference, &deployment.version)?;
        let network_profile = self.apply_network_settings(profile)?;
        let network_profile = self.apply_runtime_fakeip(&network_profile, &target, deployment)?;
        let profile = self.apply_dns_frontend_settings(
            &network_profile,
            &target,
            self.dns_settings.read().enabled,
        )?;
        let policy = sempre_converter::dns_frontend_policy(&profile, &target)?;
        let system_dns = match self
            .transparent
            .prepare_managed_dns_frontend(&deployment.core, &profile)
            .await
        {
            Ok(Some(plan)) => plan,
            Ok(None) => return Ok(None),
            Err(error) => {
                self.dns_frontend.record_failure(&error);
                self.log_supervisor(&format!("DNS frontend plan preparation failed: {error}"));
                return Ok(None);
            }
        };
        let mut plan = DnsFrontendPlan::from_policy(
            &deployment.config_hash,
            &policy,
            &system_dns.original_upstreams,
            system_dns.listen_port,
            &system_dns.listen_hosts,
            &self.dns_settings.read(),
        )?;
        if sempre_converter::cn_pre_routing(&profile) {
            match self.transparent.cn_outbound_interfaces().await {
                Ok(interfaces) => plan.config.outbound_interfaces = interfaces,
                Err(error) => self
                    .log_supervisor(&format!("DNS outbound interface discovery failed: {error}")),
            }
        }
        Ok(Some(plan))
    }

    pub(crate) async fn prepare_dns_transparent_plan(
        &self,
        document: &Document,
        deployment: &Deployment,
        reference: &CoreRef,
        runtime_config: &Path,
    ) -> Result<TransparentPlan, ManagerError> {
        let Some(profile_id) = document.active_profile_id.as_deref() else {
            return Ok(TransparentPlan::default());
        };
        let catalog = self.subscriptions.read()?;
        let profile = catalog
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| ManagerError::ProfileNotFound(profile_id.into()))?;
        let (target, _) = self.subscription_target_for(reference, &deployment.version)?;
        let network_profile = self.apply_network_settings(profile)?;
        let network_profile = self.apply_runtime_fakeip(&network_profile, &target, deployment)?;
        let profile = self.apply_dns_frontend_settings(
            &network_profile,
            &target,
            self.dns_settings.read().enabled,
        )?;
        match self
            .transparent
            .prepare(
                &deployment.core,
                &deployment.version,
                &profile,
                runtime_config,
            )
            .await
        {
            Ok(plan) => Ok(plan),
            Err(error) if operational_error(&error) => {
                self.log_supervisor(&format!(
                    "transparent proxy plan preparation failed: {error}"
                ));
                Ok(TransparentPlan::default())
            }
            Err(error) => Err(error.into()),
        }
    }
}

impl<R: VersionRunner> Manager<R> {
    pub(crate) async fn cleanup_after_core_failure(&self) -> Result<(), ManagerError> {
        self.cleanup_lifecycle(true).await
    }
}

impl DnsFrontendPlan {
    pub(crate) fn from_policy(
        deployment_hash: &str,
        policy: &DnsFrontendPolicy,
        original_upstreams: &[String],
        listen_port: u16,
        listen_hosts: &[String],
        settings: &crate::DnsSettings,
    ) -> Result<Self, ManagerError> {
        let fakeip_ranges = if policy.fakeip_enabled {
            [&policy.fakeip_ipv4_range, &policy.fakeip_ipv6_range]
                .into_iter()
                .filter_map(|range| range.parse().ok())
                .collect()
        } else {
            Vec::new()
        };
        let local_upstreams = if settings.direct_upstreams.is_empty() {
            sempre_dns::default_upstreams()
        } else {
            settings.direct_upstreams.clone()
        };
        let mut config = DnsConfig::managed_frontend(
            listen_port,
            local_upstreams,
            dns_endpoint("127.0.0.1", policy.core_listen_port),
            settings.frontend_rule_sets(),
            &settings.domestic_domains,
        )?;
        config.listen_hosts = listen_hosts.to_vec();
        config.outbound_mark = cfg!(target_os = "linux").then_some(sempre_transparent::BYPASS_MARK);
        Ok(Self {
            deployment_hash: deployment_hash.into(),
            core_upstream: config.remote_upstream.clone(),
            original_upstreams: original_upstreams.to_vec(),
            config,
            fakeip_enabled: policy.fakeip_enabled,
            fakeip_ranges,
        })
    }
}

fn operational_error(error: &sempre_transparent::TransparentError) -> bool {
    matches!(
        error,
        sempre_transparent::TransparentError::Network(_)
            | sempre_transparent::TransparentError::Io { .. }
            | sempre_transparent::TransparentError::CommandStart { .. }
            | sempre_transparent::TransparentError::CommandTimeout { .. }
            | sempre_transparent::TransparentError::CommandFailed { .. }
    )
}

fn dns_endpoint(ip: &str, port: u16) -> String {
    if ip.contains(':') {
        format!("[{ip}]:{port}")
    } else {
        format!("{ip}:{port}")
    }
}

#[cfg(test)]
#[path = "dns_runtime_tests.rs"]
mod tests;
