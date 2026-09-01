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

#[cfg(test)]
mod tests {
    use sempre_state::{Layout, Store};

    use super::*;

    #[test]
    fn policy_round_trips_by_compiled_config_hash() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager =
            Manager::with_runner(Store::new(Layout::at(root.path())), crate::ProcessRunner)
                .expect("manager");
        let policy = DnsFrontendPolicy {
            enabled: true,
            fakeip_enabled: true,
            complete: true,
            core_listen_port: 1053,
            ..DnsFrontendPolicy::default()
        };
        manager
            .save_dns_frontend_policy("abc", &policy)
            .expect("save policy");
        let data =
            std::fs::read(manager.store.layout().dns_frontend_policy("abc")).expect("read policy");
        assert_eq!(
            serde_json::from_slice::<DnsFrontendPolicy>(&data).expect("decode policy"),
            policy
        );
    }
}
