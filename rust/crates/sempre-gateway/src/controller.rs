use chrono::Utc;
use sempre_state::Layout;
use tokio::sync::Mutex;

use crate::{Config, GatewayError, LeaseView, RuntimeStatus, Store, dhcp::DhcpServer};

pub struct Controller {
    store: Store,
    runtime: std::sync::Arc<Mutex<Runtime>>,
}

#[derive(Default)]
struct Runtime {
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
            runtime: std::sync::Arc::new(Mutex::new(Runtime::default())),
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
        if !config.dhcp.enabled {
            return Ok(());
        }
        let result = DhcpServer::start(config).await;
        match result {
            Ok(dhcp) => {
                let mut runtime = self.runtime.lock().await;
                runtime.dhcp = Some(dhcp);
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
        let dhcp = {
            let mut runtime = self.runtime.lock().await;
            runtime.started_at = None;
            runtime.dhcp.take()
        };
        if let Some(dhcp) = dhcp {
            dhcp.stop().await;
        }
    }

    pub async fn runtime_status(&self) -> RuntimeStatus {
        let runtime = self.runtime.lock().await;
        RuntimeStatus {
            dhcp_running: runtime.dhcp.is_some(),
            started_at: runtime.started_at,
            dhcp_leases: runtime
                .dhcp
                .as_ref()
                .map_or_else(Vec::new, DhcpServer::leases),
            last_error: runtime.last_error.clone(),
        }
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

#[cfg(test)]
mod tests {
    use sempre_state::Layout;

    use super::*;

    #[tokio::test]
    async fn disabled_services_start_and_stop_idempotently() {
        let root = tempfile::tempdir().expect("temporary directory");
        let controller = Controller::new(&Layout::at(root.path())).expect("controller");
        controller.start().await.expect("start");
        controller.stop().await;
    }
}
