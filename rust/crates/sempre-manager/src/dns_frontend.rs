use sempre_converter::DnsFrontendPolicy;

use crate::{Manager, ManagerError, VersionRunner, subscription::RenderedProfile};

impl<R: VersionRunner> Manager<R> {
    pub(crate) fn save_rendered_runtime_policy(
        &self,
        rendered: &RenderedProfile,
    ) -> Result<(), ManagerError> {
        let hash = &rendered.render.artifact_hash;
        if let Some(policy) = &rendered.dns_frontend_policy {
            self.save_dns_frontend_policy(hash, policy)?;
        }
        let settings = self.dns_settings.read();
        if rendered.target.core == "sing-box" && settings.enabled {
            crate::dns_rule_files::save_snapshot(
                &self.store.layout().config("sing-box", hash),
                &rendered.render.content,
                &settings,
            )?;
        }
        Ok(())
    }

    pub(crate) fn save_dns_frontend_policy(
        &self,
        config_hash: &str,
        policy: &DnsFrontendPolicy,
    ) -> Result<(), ManagerError> {
        let mut data = serde_json::to_vec_pretty(policy).map_err(|error| {
            ManagerError::InvalidOperation(format!("encode DNS frontend policy: {error}"))
        })?;
        data.push(b'\n');
        sempre_state::write_atomic(
            &self.store.layout().dns_frontend_policy(config_hash),
            &data,
            0o600,
        )
        .map_err(|error| ManagerError::io("write DNS frontend policy", error))
    }
}
