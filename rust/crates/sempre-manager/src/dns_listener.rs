use std::sync::Arc;

use sempre_dns::{DnsConfig, DnsError, DnsRuntimePolicy, DnsService};
use serde::{Deserialize, Serialize};

const STANDARD_DNS_PORT: u16 = 53;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DnsPort53Status {
    pub listening: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DnsFrontendStatus {
    pub enabled: bool,
    pub running: bool,
    pub core_dns_healthy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_53: Option<DnsPort53Status>,
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

pub(crate) async fn start_dns_service(
    config: DnsConfig,
    policy: Arc<dyn DnsRuntimePolicy>,
) -> Result<(DnsService, Option<DnsPort53Status>), DnsError> {
    if !cfg!(target_os = "macos") {
        return DnsService::start_with_policy(config, policy)
            .await
            .map(|service| (service, None));
    }
    if config.listen_port == STANDARD_DNS_PORT {
        return DnsService::start_with_policy(config, policy)
            .await
            .map(|service| {
                (
                    service,
                    Some(DnsPort53Status {
                        listening: true,
                        error: String::new(),
                    }),
                )
            });
    }
    let (service, error) =
        DnsService::start_with_policy_and_optional_loopback_port(config, policy, STANDARD_DNS_PORT)
            .await?;
    Ok((
        service,
        Some(DnsPort53Status {
            listening: error.is_none(),
            error: error.unwrap_or_default(),
        }),
    ))
}
