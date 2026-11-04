//! On `OpenWrt` Sempre maintains sing-box and its routing rules, and leaves the rest of
//! the router alone. Features that would manage more of the host are refused here.
use sempre_state::Document;

use crate::{Manager, ManagerError, NetworkMode, NetworkSettings, VersionRunner};

impl<R: VersionRunner> Manager<R> {
    pub(crate) fn reject_on_openwrt(&self, feature: &str) -> Result<(), ManagerError> {
        if self.store.layout().openwrt {
            return Err(ManagerError::InvalidOperation(format!(
                "{feature} is not available on OpenWrt"
            )));
        }
        Ok(())
    }

    pub(crate) fn validate_network_candidate(
        &self,
        candidate: &NetworkSettings,
        document: &Document,
    ) -> Result<(), ManagerError> {
        if candidate.mode == NetworkMode::Gateway {
            self.reject_on_openwrt("Gateway mode")?;
        }
        if candidate.automatic_switching {
            self.reject_on_openwrt("Automatic network switching")?;
        }
        candidate.validate_intranet(document)
    }

    /// Settings written before these limits, or edited by hand, have no effect on a router.
    pub fn network_settings(&self) -> NetworkSettings {
        let mut settings = self.network_settings.read();
        if self.store.layout().openwrt {
            settings.mode = NetworkMode::Local;
            settings.automatic_switching = false;
        }
        settings
    }
}

#[cfg(test)]
mod tests {
    use sempre_state::{Layout, Store};

    use super::*;

    fn manager(layout: Layout) -> crate::Manager {
        Manager::new(Store::new(layout)).expect("manager")
    }

    #[test]
    fn a_router_refuses_features_beyond_the_core_and_its_rules() {
        let root = tempfile::tempdir().expect("root");
        let manager = manager(Layout::openwrt_at(root.path()));
        let document = manager.store.read().expect("state");
        for candidate in [
            NetworkSettings {
                mode: NetworkMode::Gateway,
                ..NetworkSettings::default()
            },
            NetworkSettings {
                automatic_switching: true,
                ..NetworkSettings::default()
            },
        ] {
            assert!(
                manager
                    .validate_network_candidate(&candidate, &document)
                    .is_err()
            );
        }
        assert!(
            manager
                .update_gateway(&sempre_gateway::Config::default())
                .is_err()
        );
        assert!(!manager.dns_settings.read().enabled);
    }

    #[test]
    fn stored_settings_cannot_enable_them_on_a_router() {
        let root = tempfile::tempdir().expect("root");
        let manager = manager(Layout::openwrt_at(root.path()));
        manager
            .network_settings
            .replace(NetworkSettings {
                mode: NetworkMode::Gateway,
                automatic_switching: true,
                ..NetworkSettings::default()
            })
            .expect("settings");
        let effective = manager.network_settings();
        assert_eq!(effective.mode, NetworkMode::Local);
        assert!(!effective.automatic_switching);
    }

    #[test]
    fn other_hosts_keep_these_features() {
        let root = tempfile::tempdir().expect("root");
        let manager = manager(Layout::at(root.path()));
        assert!(manager.reject_on_openwrt("Tunnels").is_ok());
    }
}
