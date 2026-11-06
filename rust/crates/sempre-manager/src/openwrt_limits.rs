//! On `OpenWrt` Sempre maintains sing-box and its routing rules, and leaves the rest of
//! the router alone. Features that would manage more of the host are refused here.
use sempre_state::Document;

use crate::{
    IntranetRoutingMode, Manager, ManagerError, NetworkMode, NetworkSettings, VersionRunner,
};

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
        // Both route through a TUN device, and gateway capture requires TProxy.
        if candidate.cn_direct_routes {
            self.reject_on_openwrt("CN route splitting")?;
        }
        if candidate.intranet_routing_mode == IntranetRoutingMode::Proxy {
            self.reject_on_openwrt("Proxied intranet routing")?;
        }
        candidate.validate_intranet(document)
    }

    /// Without an active profile there is nothing compiled locally either.
    pub(crate) fn active_profile_is_remote(&self) -> Result<bool, ManagerError> {
        let Some(id) = self.store.read()?.active_profile_id else {
            return Ok(true);
        };
        let catalog = self.subscriptions.read()?;
        Ok(catalog
            .profiles
            .iter()
            .find(|profile| profile.id == id)
            .is_none_or(|profile| crate::subscription::profile_mode(profile) == "remote"))
    }

    /// Settings written before these limits, or edited by hand, have no effect on a router.
    pub fn network_settings(&self) -> NetworkSettings {
        let mut settings = self.network_settings.read();
        if self.store.layout().openwrt {
            settings.mode = NetworkMode::Local;
            settings.automatic_switching = false;
            settings.cn_direct_routes = false;
            settings.intranet_routing_mode = IntranetRoutingMode::Direct;
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
            NetworkSettings {
                cn_direct_routes: true,
                ..NetworkSettings::default()
            },
            NetworkSettings {
                intranet_routing_mode: IntranetRoutingMode::Proxy,
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

    #[tokio::test]
    async fn a_router_only_runs_the_core_it_was_installed_with() {
        let root = tempfile::tempdir().expect("root");
        let manager = manager(Layout::openwrt_at(root.path()));
        assert!(manager.install_core("mihomo").await.is_err());
        assert!(manager.update_cores("sing-box@stable").await.is_err());
        assert!(manager.install_core("sing-box@stable").await.is_err());
        let error = manager.select_core("mihomo").await.expect_err("select");
        assert!(error.to_string().contains("not available on OpenWrt"));
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
                cn_direct_routes: true,
                intranet_routing_mode: IntranetRoutingMode::Proxy,
                ..NetworkSettings::default()
            })
            .expect("settings");
        let effective = manager.network_settings();
        assert_eq!(effective.mode, NetworkMode::Local);
        assert!(!effective.automatic_switching);
        assert!(!effective.cn_direct_routes);
        assert_eq!(effective.intranet_routing_mode, IntranetRoutingMode::Direct);
    }

    #[test]
    fn a_router_leaves_the_private_address_rule_to_the_compiled_selector() {
        let root = tempfile::tempdir().expect("root");
        let profile = sempre_converter::Profile::default();
        let router = manager(Layout::openwrt_at(root.path()))
            .apply_network_settings(&profile)
            .expect("router profile");
        assert!(!router.extra.contains_key("sempre_intranet_routing_mode"));
        let desktop_root = tempfile::tempdir().expect("root");
        let desktop = manager(Layout::at(desktop_root.path()))
            .apply_network_settings(&profile)
            .expect("desktop profile");
        assert!(desktop.extra.contains_key("sempre_intranet_routing_mode"));
    }

    #[test]
    fn other_hosts_keep_these_features() {
        let root = tempfile::tempdir().expect("root");
        let manager = manager(Layout::at(root.path()));
        assert!(manager.reject_on_openwrt("Tunnels").is_ok());
    }
}
