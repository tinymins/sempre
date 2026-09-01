use std::{
    net::IpAddr,
    path::Path,
    sync::{Arc, RwLock},
    time::Duration,
};

use ipnet::IpNet;
use sempre_converter::DnsFrontendPolicy;
use sempre_core::CoreRef;
use sempre_gateway::{DnsConfig, DnsRuntimePolicy, DnsService, managed_probe_names, probe_dns};
use sempre_state::{Deployment, Document};
use sempre_transparent::Plan as TransparentPlan;
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, time::sleep};

use crate::{Manager, ManagerError, ValidationRunner, VersionRunner, supervisor::RuntimePlan};

const PROBE_INTERVAL: Duration = Duration::from_millis(200);

pub(crate) struct DnsFrontendRuntime {
    running: Mutex<Option<RunningFrontend>>,
    status: RwLock<DnsFrontendStatus>,
    policy: Arc<dyn DnsRuntimePolicy>,
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
    deployment_hash: String,
    service: DnsService,
}

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
    pub(crate) fn new(policy: Arc<dyn DnsRuntimePolicy>) -> Arc<Self> {
        Arc::new(Self {
            running: Mutex::new(None),
            status: RwLock::new(DnsFrontendStatus::default()),
            policy,
        })
    }

    pub(crate) async fn activate(
        &self,
        plan: &RuntimePlan,
        timeout: Duration,
    ) -> Result<(), ManagerError> {
        let Some(frontend) = plan.dns_frontend.as_ref() else {
            sleep(timeout).await;
            return Ok(());
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
            return Err(error);
        }
        self.status
            .write()
            .expect("DNS frontend status")
            .core_dns_healthy = true;
        self.start(frontend).await?;
        if let Err(error) = self.probe_frontend(frontend, timeout).await {
            self.stop().await;
            return Err(error);
        }
        self.status
            .write()
            .expect("DNS frontend status")
            .last_error
            .clear();
        Ok(())
    }

    pub(crate) async fn stop(&self) {
        if let Some(running) = self.running.lock().await.take() {
            running.service.stop().await;
        }
        *self.status.write().expect("DNS frontend status") = DnsFrontendStatus::default();
    }

    pub(crate) fn configure(&self, plan: &DnsFrontendPlan) {
        let running = self.status.read().expect("DNS frontend status").running;
        *self.status.write().expect("DNS frontend status") = DnsFrontendStatus {
            enabled: true,
            running,
            core_dns_healthy: false,
            mode: if plan.fakeip_enabled {
                "fake-ip"
            } else {
                "real-ip"
            }
            .into(),
            core_upstream: plan.core_upstream.clone(),
            original_upstreams: plan.original_upstreams.clone(),
            direct_upstreams: plan.config.local_upstreams.clone(),
            domestic_domain_source: sempre_gateway::DOMESTIC_DOMAIN_SOURCE.into(),
            domestic_domain_sha256: sempre_gateway::DOMESTIC_DOMAIN_SHA256.into(),
            domestic_domain_count: sempre_gateway::DOMESTIC_DOMAIN_COUNT,
            last_error: String::new(),
        };
    }

    pub(crate) fn status(&self) -> DnsFrontendStatus {
        self.status.read().expect("DNS frontend status").clone()
    }

    pub(crate) fn record_failure(&self, error: &impl ToString) {
        let mut status = self.status.write().expect("DNS frontend status");
        status.core_dns_healthy = false;
        status.last_error = error.to_string();
    }

    async fn start(&self, plan: &DnsFrontendPlan) -> Result<(), ManagerError> {
        let mut running = self.running.lock().await;
        if let Some(current) = running.as_ref() {
            if current.deployment_hash == plan.deployment_hash {
                return Ok(());
            }
            return Err(ManagerError::InvalidOperation(
                "DNS frontend is still owned by a different deployment".into(),
            ));
        }
        let service =
            DnsService::start_with_policy(plan.config.clone(), Arc::clone(&self.policy)).await?;
        *running = Some(RunningFrontend {
            deployment_hash: plan.deployment_hash.clone(),
            service,
        });
        self.status.write().expect("DNS frontend status").running = true;
        Ok(())
    }

    async fn probe_frontend(
        &self,
        plan: &DnsFrontendPlan,
        timeout: Duration,
    ) -> Result<(), ManagerError> {
        let upstream = dns_endpoint("127.0.0.1", plan.config.listen_port);
        wait_for_answer(plan, &upstream, &plan.local_probe, false, timeout).await?;
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
        let profile = sempre_converter::apply_dns_frontend_settings(
            profile,
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

impl DnsFrontendPlan {
    pub(crate) fn from_policy(
        deployment_hash: &str,
        policy: &DnsFrontendPolicy,
        original_upstreams: &[String],
        listen_port: u16,
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
            original_upstreams
                .iter()
                .map(|ip| dns_endpoint(ip, 53))
                .collect()
        } else {
            settings.direct_upstreams.clone()
        };
        let config = DnsConfig::managed_frontend(
            listen_port,
            local_upstreams,
            dns_endpoint("127.0.0.1", policy.core_listen_port),
            settings.frontend_rule_sets(),
        )?;
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
        match tokio::time::timeout(Duration::from_secs(2), probe_dns(upstream, name, "A")).await {
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
