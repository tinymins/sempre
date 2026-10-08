use sempre_converter::DnsFrontendPolicy;

use crate::{Manager, ManagerError, VersionRunner};

impl<R: VersionRunner> Manager<R> {
    pub(crate) fn save_optional_dns_frontend_policy(
        &self,
        config_hash: &str,
        policy: Option<&DnsFrontendPolicy>,
    ) -> Result<(), ManagerError> {
        policy.map_or(Ok(()), |policy| {
            self.save_dns_frontend_policy(config_hash, policy)
        })
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
