use ipnet::IpNet;
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
    pub home_cidrs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_cidr: Option<String>,
}

impl<R: VersionRunner> Manager<R> {
    pub fn private_access_status(&self) -> Result<PrivateAccessStatus, ManagerError> {
        Ok(Self::private_access_status_value(&self.store.read()?))
    }

    pub(crate) fn private_access_status_value(document: &Document) -> PrivateAccessStatus {
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
        evaluate(build, active, observed, probe_error)
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
) -> PrivateAccessStatus {
    let connectors = configured_connectors(&build.private_access_policy)
        .into_iter()
        .map(|(tag, home_cidrs)| {
            let matched_cidr = observed.addresses.iter().find_map(|address| {
                let address = address.parse::<IpNet>().ok()?.addr();
                home_cidrs.iter().find_map(|cidr| {
                    cidr.parse::<IpNet>()
                        .ok()
                        .filter(|network| network.contains(&address))
                        .map(|_| cidr.clone())
                })
            });
            let mode = if !active {
                "inactive"
            } else if matched_cidr.is_some() {
                "direct"
            } else if observed.supported && !observed.addresses.is_empty() {
                "wireguard"
            } else {
                "unknown"
            };
            PrivateAccessConnectorStatus {
                tag,
                mode: mode.into(),
                home_cidrs,
                matched_cidr,
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
            let home_cidrs = home
                .get("addressCidrs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            (!home_cidrs.is_empty()).then(|| {
                let tag = connector
                    .get("tag")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map_or_else(|| format!("private-access-{}", index + 1), str::to_owned);
                (tag, home_cidrs)
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
                    "homeNetwork": { "enabled": true, "addressCidrs": ["10.8.28.0/24"] }
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
            },
            None,
        );
        assert_eq!(status.connectors[0].mode, "direct");
        assert_eq!(
            status.connectors[0].matched_cidr.as_deref(),
            Some("10.8.28.0/24")
        );

        let away = evaluate(
            &build(),
            true,
            DefaultInterface {
                supported: true,
                name: "en0".into(),
                addresses: vec!["10.44.7.169/20".into()],
            },
            None,
        );
        assert_eq!(away.connectors[0].mode, "wireguard");
    }

    #[test]
    fn reports_inactive_when_the_core_is_not_using_the_policy() {
        let status = evaluate(&build(), false, DefaultInterface::default(), None);
        assert_eq!(status.connectors[0].mode, "inactive");
    }
}
