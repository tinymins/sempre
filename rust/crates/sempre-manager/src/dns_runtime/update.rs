use sempre_dns::{DnsConfig, managed_probe_names};

use super::DnsFrontendRuntime;
use crate::{DnsSettings, ManagerError};

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

    pub(crate) async fn update_rules(
        &self,
        settings: &DnsSettings,
    ) -> Result<Option<DnsConfig>, ManagerError> {
        let mut running = self.running.lock().await;
        let Some(current) = running.as_mut() else {
            return Ok(None);
        };
        let previous = current.plan.config.clone();
        let mut config = previous.clone();
        config.rule_sets = DnsConfig::managed_rule_sets(
            settings.frontend_rule_sets(),
            &settings.domestic_domains,
        )?;
        config
            .local_upstreams
            .clone_from(&settings.direct_upstreams);
        let (local_probe, remote_probe) = managed_probe_names(&config)?;
        current.service.update(config.clone())?;
        current.plan.local_probe = local_probe;
        current.plan.remote_probe = remote_probe;
        current.plan.config = config;
        self.status
            .write()
            .expect("DNS frontend status")
            .direct_upstreams
            .clone_from(&settings.direct_upstreams);
        Ok(Some(previous))
    }

    pub(crate) async fn restore_rules(
        &self,
        config: Option<DnsConfig>,
    ) -> Result<(), ManagerError> {
        let Some(config) = config else {
            return Ok(());
        };
        let mut running = self.running.lock().await;
        let Some(current) = running.as_mut() else {
            return Err(ManagerError::InvalidOperation(
                "DNS frontend stopped during rule set publication".into(),
            ));
        };
        let (local_probe, remote_probe) = managed_probe_names(&config)?;
        current.service.update(config.clone())?;
        current.plan.local_probe = local_probe;
        current.plan.remote_probe = remote_probe;
        self.status
            .write()
            .expect("DNS frontend status")
            .direct_upstreams
            .clone_from(&config.local_upstreams);
        current.plan.config = config;
        Ok(())
    }
}
