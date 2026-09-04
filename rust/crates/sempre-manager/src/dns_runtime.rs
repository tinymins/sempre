use std::{
    net::IpAddr,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::Duration,
};

use ipnet::IpNet;
use sempre_converter::DnsFrontendPolicy;
use sempre_core::CoreRef;
use sempre_dns::{DnsConfig, DnsRuntimePolicy, DnsService, managed_probe_names, probe_dns};
use sempre_state::{Deployment, Document};
use sempre_transparent::Plan as TransparentPlan;
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, time::sleep};

use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

const PROBE_INTERVAL: Duration = Duration::from_millis(200);

pub(crate) struct DnsFrontendRuntime {
    running: Mutex<Option<RunningFrontend>>,
    status: RwLock<DnsFrontendStatus>,
    policy: Arc<dyn DnsRuntimePolicy>,
    resources: Option<PathBuf>,
    capture_error: crate::dns_capture::CaptureError,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DnsFrontendStatus {
    pub enabled: bool,
    pub running: bool,
    pub core_dns_healthy: bool,
    pub mode: String,
    pub core_upstream: String,
    pub original_upstreams: Vec<String>,
    pub direct_upstreams: Vec<String>,
    pub domestic_domain_source: String,
    pub domestic_domain_sha256: String,
    pub domestic_domain_count: usize,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_error: String,
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
    local_probe: String,
    remote_probe: String,
}

impl DnsFrontendRuntime {
    pub(crate) async fn update_upstreams(&self, upstreams: &[String]) -> Result<(), ManagerError> {
        let mut running = self.running.lock().await;
        if let Some(current) = running.as_mut() {
            if current.plan.config.local_upstreams == upstreams {
                return Ok(());
            }
            let mut config = current.plan.config.clone();
            config.local_upstreams = upstreams.to_vec();
            current.service.update(config.clone())?;
            current.plan.config = config;
            self.status
                .write()
                .expect("DNS frontend status")
                .direct_upstreams = upstreams.to_vec();
        }
        Ok(())
    }

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
        timeout: Duration,
    ) -> Result<(), ManagerError> {
        let Some(frontend) = frontend else {
            return Ok(());
        };
        self.start_if_missing(frontend).await?;
        let current = self
            .running
            .lock()
            .await
            .as_ref()
            .map(|running| running.plan.clone())
            .expect("prepared DNS frontend");
        if let Err(error) = self.probe_local(&current, timeout).await {
            self.stop().await;
            return Err(error);
        }
        Ok(())
    }

    pub(crate) async fn activate_core(
        &self,
        frontend: Option<&DnsFrontendPlan>,
        timeout: Duration,
    ) {
        let Some(frontend) = frontend else {
            return;
        };
        if let Err(error) = wait_for_answer(
            frontend,
            &frontend.core_upstream,
            &frontend.remote_probe,
            frontend.fakeip_enabled,
            timeout,
        )
        .await
        {
            self.record_failure(&error);
            return;
        }
        if let Err(error) = self.promote(frontend, timeout).await {
            self.record_failure(&error);
            return;
        }
        self.status
            .write()
            .expect("DNS frontend status")
            .core_dns_healthy = true;
        self.status
            .write()
            .expect("DNS frontend status")
            .last_error
            .clear();
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

    fn configure(&self, plan: &DnsFrontendPlan, core_dns_healthy: bool) {
        *self.status.write().expect("DNS frontend status") = DnsFrontendStatus {
            enabled: true,
            running: true,
            core_dns_healthy,
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
        status.core_dns_healthy = false;
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
        let service =
            DnsService::start_with_policy(plan.config.clone(), Arc::clone(&self.policy)).await?;
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
        self.configure(plan, false);
        Ok(())
    }

    async fn promote(&self, plan: &DnsFrontendPlan, timeout: Duration) -> Result<(), ManagerError> {
        let previous = {
            let mut running = self.running.lock().await;
            let current = running.as_mut().expect("prepared DNS frontend");
            if current.plan.deployment_hash == plan.deployment_hash
                && current.plan.config == plan.config
            {
                current.plan.clone()
            } else {
                let previous = current.plan.clone();
                current.service.update(plan.config.clone())?;
                current.plan = plan.clone();
                previous
            }
        };
        self.configure(plan, false);
        if let Err(error) = self.probe_frontend(plan, timeout).await {
            let mut running = self.running.lock().await;
            let current = running.as_mut().expect("prepared DNS frontend");
            let _ = current.service.update(previous.config.clone());
            current.plan = previous.clone();
            self.configure(&previous, false);
            return Err(error);
        }
        Ok(())
    }

    async fn probe_local(
        &self,
        plan: &DnsFrontendPlan,
        timeout: Duration,
    ) -> Result<(), ManagerError> {
        let upstream = dns_endpoint("127.0.0.1", plan.config.listen_port);
        wait_for_answer(plan, &upstream, &plan.local_probe, false, timeout).await?;
        Ok(())
    }

    async fn probe_frontend(
        &self,
        plan: &DnsFrontendPlan,
        timeout: Duration,
    ) -> Result<(), ManagerError> {
        self.probe_local(plan, timeout).await?;
        let upstream = dns_endpoint("127.0.0.1", plan.config.listen_port);
        wait_for_answer(
            plan,
            &upstream,
            &plan.remote_probe,
            plan.fakeip_enabled,
            timeout,
        )
        .await
    }
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
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
        let profile = self.apply_dns_frontend_settings(
            &network_profile,
            &target,
            self.dns_settings.read().enabled,
        )?;
        let Some(system_dns) = self
            .transparent
            .prepare_managed_dns_frontend(&deployment.core, &profile)
            .await?
        else {
            return Ok(None);
        };
        let policy = sempre_converter::dns_frontend_policy(&profile, &target)?;
        if policy.core_listen_port != system_dns.core_listen_port {
            return Err(ManagerError::InvalidOperation(
                "managed DNS frontend and core listener ports do not match".into(),
            ));
        }
        crate::dns_port::ensure_available(policy.core_listen_port)?;
        Ok(Some(DnsFrontendPlan::from_policy(
            &deployment.config_hash,
            &policy,
            &system_dns.original_upstreams,
            system_dns.listen_port,
            &system_dns.listen_hosts,
            &self.dns_settings.read(),
        )?))
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
        let profile = self.apply_dns_frontend_settings(
            &network_profile,
            &target,
            self.dns_settings.read().enabled,
        )?;
        Ok(self
            .transparent
            .prepare(
                &deployment.core,
                &deployment.version,
                &profile,
                runtime_config,
            )
            .await?)
    }
}

impl<R: VersionRunner> Manager<R> {
    pub(crate) async fn cleanup_after_core_failure(
        &self,
    ) -> Result<(), sempre_transparent::TransparentError> {
        self.stop_gateway().await;
        if self.dns_frontend.status().running {
            self.transparent.cleanup_runtime_network().await
        } else {
            self.dns_frontend.stop().await;
            self.transparent.cleanup().await
        }
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
        if !policy.enabled || !policy.complete {
            return Err(ManagerError::InvalidOperation(
                "managed DNS frontend policy is disabled or incomplete".into(),
            ));
        }
        let mut fakeip_ranges = vec![parse_range(&policy.fakeip_ipv4_range)?];
        if !policy.fakeip_ipv6_range.trim().is_empty() {
            fakeip_ranges.push(parse_range(&policy.fakeip_ipv6_range)?);
        }
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
        )?;
        config.listen_hosts = listen_hosts.to_vec();
        config.outbound_mark = cfg!(target_os = "linux").then_some(sempre_transparent::BYPASS_MARK);
        let (local_probe, remote_probe) = managed_probe_names(&config)?;
        Ok(Self {
            deployment_hash: deployment_hash.into(),
            core_upstream: config.remote_upstream.clone(),
            original_upstreams: original_upstreams.to_vec(),
            local_probe,
            remote_probe,
            config,
            fakeip_enabled: policy.fakeip_enabled,
            fakeip_ranges,
        })
    }

    fn valid_answer(&self, expected_fake: bool, addresses: &[IpAddr]) -> bool {
        if addresses.is_empty() {
            return false;
        }
        let fake = addresses.iter().all(|address| {
            self.fakeip_ranges
                .iter()
                .any(|range| range.contains(address))
        });
        fake == expected_fake
    }
}

async fn wait_for_answer(
    plan: &DnsFrontendPlan,
    upstream: &str,
    name: &str,
    expected_fake: bool,
    timeout: Duration,
) -> Result<(), ManagerError> {
    let started = tokio::time::Instant::now();
    let mut last_error = String::new();
    while started.elapsed() < timeout {
        let attempt_timeout = timeout
            .checked_sub(started.elapsed())
            .unwrap_or_default()
            .min(Duration::from_secs(2));
        match tokio::time::timeout(attempt_timeout, probe_dns(upstream, name, "A")).await {
            Err(_) => last_error = "DNS probe timed out".into(),
            Ok(Ok(result))
                if result.response_code == 0
                    && plan.valid_answer(expected_fake, &result.addresses) =>
            {
                return Ok(());
            }
            Ok(Ok(result)) => {
                last_error = format!(
                    "DNS probe returned code {} and addresses {:?}",
                    result.response_code, result.addresses
                );
            }
            Ok(Err(error)) => last_error = error.to_string(),
        }
        sleep(PROBE_INTERVAL).await;
    }
    Err(ManagerError::RuntimeNotReady(format!(
        "DNS health probe {name} via {upstream} failed: {last_error}"
    )))
}

fn parse_range(value: &str) -> Result<IpNet, ManagerError> {
    value.parse().map_err(|_| {
        ManagerError::InvalidOperation(format!("invalid managed FakeIP range {value:?}"))
    })
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
