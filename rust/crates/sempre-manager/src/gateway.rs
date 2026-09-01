use sempre_gateway::{
    Config, HostApplyRequest, HostPlan, Status, apply_host_plan, build_host_plan,
    validation_messages,
};
use sempre_state::DesiredState;

use crate::{Manager, ManagerError, VersionRunner};

impl<R: VersionRunner> Manager<R> {
    pub async fn gateway_status(&self) -> Result<Status, ManagerError> {
        let config = self.gateway.read()?;
        let transparent_proxy = self.active_transparent_proxy();
        Ok(Status {
            validation_errors: validation_messages(&config),
            config,
            runtime: self.gateway.runtime_status().await,
            inventory: sempre_network::inventory().unwrap_or_default(),
            transparent_proxy,
            host_plan_available: true,
        })
    }

    pub fn update_gateway(&self, config: &Config) -> Result<(Config, bool), ManagerError> {
        let saved = self.gateway.update(config)?;
        let reload_requested = self.store.read()?.desired_state == DesiredState::Running;
        if reload_requested {
            self.request_runtime_reload();
        }
        Ok((saved, reload_requested))
    }

    pub fn validate_gateway(config: &Config) -> Vec<String> {
        validation_messages(config)
    }

    pub fn gateway_host_plan(config: Config) -> Result<HostPlan, ManagerError> {
        Ok(build_host_plan(config)?)
    }

    pub async fn apply_gateway_host_plan(
        &self,
        request: HostApplyRequest,
    ) -> Result<HostPlan, ManagerError> {
        Ok(apply_host_plan(request).await?)
    }

    pub async fn revoke_gateway_lease(&self, mac: &str) -> Result<(), ManagerError> {
        Ok(self.gateway.revoke_lease(mac).await?)
    }

    pub(crate) async fn start_gateway(&self) -> Result<(), ManagerError> {
        if self.network_settings().mode != crate::NetworkMode::Gateway {
            self.gateway.stop().await;
            return Ok(());
        }
        Ok(self.gateway.start().await?)
    }

    pub(crate) async fn stop_gateway(&self) {
        self.gateway.stop().await;
    }

    fn active_transparent_proxy(&self) -> Option<serde_json::Value> {
        let document = self.store.read().ok()?;
        let id = document.active_profile_id.as_deref()?;
        let catalog = self.subscriptions.read().ok()?;
        let profile = catalog.profiles.iter().find(|profile| profile.id == id)?;
        serde_json::to_value(&profile.transparent_proxy).ok()
    }
}
