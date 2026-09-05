use sempre_network::DefaultInterface;
use sempre_state::{ConfigBuild, Document, RuntimeState};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Manager, ManagerError, VersionRunner};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrivateAccessStatus {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub profile_id: String,
    pub profile_revision: u64,
    pub active: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub interface: String,
    pub interface_addresses: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_error: Option<String>,
    pub connectors: Vec<PrivateAccessConnectorStatus>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrivateAccessConnectorStatus {
    pub tag: String,
    pub mode: String,
    pub home_networks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_network: Option<String>,
}

impl<R: VersionRunner> Manager<R> {
    pub fn private_access_status(&self) -> Result<PrivateAccessStatus, ManagerError> {
        Ok(self.private_access_status_value(&self.store.read()?))
    }

    pub(crate) fn private_access_status_value(&self, document: &Document) -> PrivateAccessStatus {
        let Some(build) = applied_build(document) else {
            return PrivateAccessStatus::default();
        };
        let active = document.runtime.pid.is_some()
            && matches!(
                document.runtime.state,
                RuntimeState::Starting
                    | RuntimeState::Running
                    | RuntimeState::Stopping
                    | RuntimeState::Restarting
            );
        let (observed, probe_error) = match sempre_network::default_interface() {
            Ok(value) => (value, None),
            Err(error) => (DefaultInterface::default(), Some(error.to_string())),
        };
        let settings = self.network_settings.read();
        evaluate(
            build,
            active,
            observed,
            probe_error,
            settings.automatic_switching,
            &settings.known_networks,
        )
    }
}

fn applied_build(document: &Document) -> Option<&ConfigBuild> {
    let runtime_hash = document.runtime.config_hash.as_deref()?;
    if let Some(active) = document
        .active
        .as_ref()
        .filter(|deployment| deployment.config_hash == runtime_hash)
    {
        return document.config_builds.get(&active.core);
    }
    document
        .previous
        .as_ref()
        .filter(|deployment| deployment.config_hash == runtime_hash)
        .and(document.previous_config_build.as_ref())
}

fn evaluate(
    build: &ConfigBuild,
    active: bool,
    observed: DefaultInterface,
    probe_error: Option<String>,
    automatic_switching: bool,
    known_networks: &[crate::KnownNetwork],
) -> PrivateAccessStatus {
    let connectors = configured_connectors(&build.private_access_policy)
        .into_iter()
        .map(|(tag, home_network_ids)| {
            let observed_mac = sempre_network::normalize_mac(&observed.gateway_mac);
            let matched_network = observed_mac.and_then(|mac| {
                known_networks.iter().find_map(|network| {
                    (home_network_ids.contains(&network.id)
                        && sempre_network::normalize_mac(&network.gateway_mac).as_deref()
                            == Some(mac.as_str()))
                    .then(|| network.name.clone())
                })
            });
            let home_networks = home_network_ids
                .iter()
                .map(|id| {
                    known_networks
                        .iter()
                        .find(|network| network.id == *id)
                        .map_or_else(|| id.clone(), |network| network.name.clone())
                })
                .collect();
            let mode = if !active {
                "inactive"
            } else if automatic_switching && matched_network.is_some() {
                "direct"
            } else if observed.supported && !observed.gateway_mac.is_empty() {
                "wireguard"
            } else {
                "unknown"
            };
            PrivateAccessConnectorStatus {
                tag,
                mode: mode.into(),
                home_networks,
                matched_network,
            }
        })
        .collect();
    PrivateAccessStatus {
        profile_id: build.profile_id.clone(),
        profile_revision: build.profile_revision,
        active,
        interface: observed.name,
        interface_addresses: observed.addresses,
        probe_error,
        connectors,
    }
}

fn configured_connectors(config: &Value) -> Vec<(String, Vec<String>)> {
    if config.get("enabled").and_then(Value::as_bool) != Some(true) {
        return Vec::new();
    }
    config
        .get("connectors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, connector)| {
            if connector.get("enabled").and_then(Value::as_bool) == Some(false)
                || connector.get("type").and_then(Value::as_str) != Some("wireguard")
            {
                return None;
            }
            let home = connector.get("homeNetwork")?;
            if home.get("enabled").and_then(Value::as_bool) != Some(true) {
                return None;
            }
            let home_network_ids = home
                .get("networkIds")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            (!home_network_ids.is_empty()).then(|| {
                let tag = connector
                    .get("tag")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map_or_else(|| format!("private-access-{}", index + 1), str::to_owned);
                (tag, home_network_ids)
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use sempre_state::ConfigBuild;
    use serde_json::json;

    use super::*;

    fn build() -> ConfigBuild {
        ConfigBuild {
            profile_id: "home".into(),
            profile_revision: 7,
            target_key: String::new(),
            runtime_key: None,
            private_access_policy: json!({
                "enabled": true,
                "connectors": [{
                    "type": "wireguard", "tag": "home-wg",
                    "homeNetwork": { "enabled": true, "networkIds": ["d286d2f8-33c5-4f1e-b871-d22a9ba91143"] }
                }]
            }),
        }
    }

    #[test]
    fn reports_direct_only_when_the_default_interface_matches_home() {
        let status = evaluate(
            &build(),
            true,
            DefaultInterface {
                supported: true,
                name: "en0".into(),
                addresses: vec!["10.8.28.19/24".into()],
                gateway: "10.8.28.1".into(),
                gateway_mac: "aa:bb:cc:dd:ee:ff".into(),
            },
            None,
            true,
            &[crate::KnownNetwork {
                id: "d286d2f8-33c5-4f1e-b871-d22a9ba91143".into(),
                name: "Home".into(),
                gateway_mac: "aa:bb:cc:dd:ee:ff".into(),
                disable_proxy: true,
            }],
        );
        assert_eq!(status.connectors[0].mode, "direct");
        assert_eq!(
            status.connectors[0].matched_network.as_deref(),
            Some("Home")
        );

        let away = evaluate(
            &build(),
            true,
            DefaultInterface {
                supported: true,
                name: "en0".into(),
                addresses: vec!["10.44.7.169/20".into()],
                gateway: "10.44.0.1".into(),
                gateway_mac: "00:11:22:33:44:55".into(),
            },
            None,
            true,
            &[],
        );
        assert_eq!(away.connectors[0].mode, "wireguard");
    }

    #[test]
    fn reports_inactive_when_the_core_is_not_using_the_policy() {
        let status = evaluate(
            &build(),
            false,
            DefaultInterface::default(),
            None,
            false,
            &[],
        );
        assert_eq!(status.connectors[0].mode, "inactive");
    }
}
