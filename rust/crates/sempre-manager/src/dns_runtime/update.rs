use sempre_dns::DnsConfig;

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
        config.rule_sets = settings.frontend_rule_sets();
        config
            .local_upstreams
            .clone_from(&settings.direct_upstreams);
        current.service.update(config.clone())?;
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
        current.service.update(config.clone())?;
        self.status
            .write()
            .expect("DNS frontend status")
            .direct_upstreams
            .clone_from(&config.local_upstreams);
        current.plan.config = config;
        Ok(())
    }
}
