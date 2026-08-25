use std::sync::Arc;

use chrono::Utc;
use sempre_state::Layout;
use tokio::sync::Mutex;

use crate::{
    Config, DnsDebugResult, GatewayError, LeaseView, RuntimeStatus, Store,
    dhcp::DhcpServer,
    dns::{DnsServer, debug_query},
    rules::resolve_rule_sets,
};

pub struct Controller {
    store: Store,
    runtime: Arc<Mutex<Runtime>>,
}

#[derive(Default)]
struct Runtime {
    dns: Option<DnsServer>,
    dhcp: Option<DhcpServer>,
    started_at: Option<chrono::DateTime<Utc>>,
    last_error: String,
}

impl Controller {
    pub fn new(layout: &Layout) -> Result<Self, GatewayError> {
        let store = Store::new(layout);
        store.initialize()?;
        Ok(Self {
            store,
            runtime: Arc::new(Mutex::new(Runtime::default())),
        })
    }

    pub fn read(&self) -> Result<Config, GatewayError> {
        self.store.read()
    }

    pub fn update(&self, config: &Config) -> Result<Config, GatewayError> {
        self.store.write(config)
    }

    pub async fn start(&self) -> Result<(), GatewayError> {
        let config = self.read()?;
        self.start_config(config).await
    }

    pub async fn start_config(&self, mut config: Config) -> Result<(), GatewayError> {
        config.normalize();
        config.validate()?;
        self.stop().await;
        if !config.dns.enabled && !config.dhcp.enabled {
            return Ok(());
        }
        let result = start_services(&config).await;
        match result {
            Ok((dns, dhcp)) => {
                let mut runtime = self.runtime.lock().await;
                runtime.dns = dns;
                runtime.dhcp = dhcp;
                runtime.started_at = Some(Utc::now());
                runtime.last_error.clear();
                Ok(())
            }
            Err(error) => {
                self.runtime.lock().await.last_error = error.to_string();
                Err(error)
            }
        }
    }

    pub async fn stop(&self) {
        let (dns, dhcp) = {
            let mut runtime = self.runtime.lock().await;
            runtime.started_at = None;
            (runtime.dns.take(), runtime.dhcp.take())
        };
        if let Some(dns) = dns {
            dns.stop().await;
        }
        if let Some(dhcp) = dhcp {
            dhcp.stop().await;
        }
    }

    pub async fn runtime_status(&self) -> RuntimeStatus {
        let runtime = self.runtime.lock().await;
        RuntimeStatus {
            dns_running: runtime.dns.is_some(),
            dhcp_running: runtime.dhcp.is_some(),
            started_at: runtime.started_at,
            dhcp_leases: runtime
                .dhcp
                .as_ref()
                .map_or_else(Vec::new, DhcpServer::leases),
            last_error: runtime.last_error.clone(),
        }
    }

    pub async fn query_dns(
        &self,
        name: &str,
        record_type: &str,
    ) -> Result<DnsDebugResult, GatewayError> {
        let config = self.read()?;
        let config = resolve_rule_sets(config.dns).await?;
        debug_query(config, name, record_type).await
    }

    pub async fn revoke_lease(&self, mac: &str) -> Result<(), GatewayError> {
        let runtime = self.runtime.lock().await;
        runtime
            .dhcp
            .as_ref()
            .ok_or_else(|| GatewayError::invalid("DHCP server is not running"))?
            .revoke(mac)
    }

    pub async fn leases(&self) -> Vec<LeaseView> {
        self.runtime
            .lock()
            .await
            .dhcp
            .as_ref()
            .map_or_else(Vec::new, DhcpServer::leases)
    }
}

async fn start_services(
    config: &Config,
) -> Result<(Option<DnsServer>, Option<DhcpServer>), GatewayError> {
    let dns = if config.dns.enabled {
        let dns_config = resolve_rule_sets(config.dns.clone()).await?;
        Some(DnsServer::start(dns_config).await?)
    } else {
        None
    };
    let dhcp = if config.dhcp.enabled {
        match DhcpServer::start(config.clone()).await {
            Ok(dhcp) => Some(dhcp),
            Err(error) => {
                if let Some(dns) = dns {
                    dns.stop().await;
                }
                return Err(error);
            }
        }
    } else {
        None
    };
    Ok((dns, dhcp))
}

#[cfg(test)]
mod tests {
    use sempre_state::Layout;

    use super::*;

    #[tokio::test]
    async fn disabled_services_start_and_stop_idempotently() {
        let root = tempfile::tempdir().expect("temporary directory");
        let controller = Controller::new(&Layout::at(root.path())).expect("controller");
        controller.start().await.expect("start");
        assert!(!controller.runtime_status().await.dns_running);
        controller.stop().await;
    }

    #[tokio::test]
    async fn dns_listener_starts_on_configured_udp_and_tcp_port() {
        let root = tempfile::tempdir().expect("temporary directory");
        let controller = Controller::new(&Layout::at(root.path())).expect("controller");
        let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("port probe");
        let port = probe.local_addr().expect("probe address").port();
        drop(probe);
        let mut config = Config::default();
        config.dns.enabled = true;
        config.dns.listen_hosts = vec!["127.0.0.1".into()];
        config.dns.listen_port = port;
        controller.start_config(config).await.expect("start DNS");
        assert!(controller.runtime_status().await.dns_running);
        controller.stop().await;
        assert!(!controller.runtime_status().await.dns_running);
    }
}
