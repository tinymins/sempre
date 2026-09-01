use chrono::{DateTime, Utc};
use sempre_converter::{CustomNode, Profile};
use sempre_core::{AutoConfigRequirements, CoreRef, features};
use sempre_state::Document;
use serde::Serialize;

use crate::{CoreChange, Manager, ManagerError, ValidationRunner, VersionRunner};

#[derive(Clone, Debug, Serialize)]
pub struct AutoConfigCandidate {
    #[serde(flatten)]
    pub candidate: sempre_core::AutoConfigCandidate,
    pub installed: bool,
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct AutoConfigCheck {
    pub id: String,
    pub status: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AutoConfigReport {
    pub checked_at: DateTime<Utc>,
    pub platform: String,
    pub architecture: String,
    pub policy_version: &'static str,
    pub requirements: AutoConfigRequirements,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<AutoConfigCandidate>,
    pub candidates: Vec<AutoConfigCandidate>,
    pub checks: Vec<AutoConfigCheck>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AutoConfigApplyResult {
    pub recommendation: AutoConfigCandidate,
    pub changes: Vec<CoreChange>,
}

impl<R: VersionRunner> Manager<R> {
    pub fn diagnose_core_configuration(&self) -> Result<AutoConfigReport, ManagerError> {
        let document = self.store.read()?;
        let catalog = self.subscriptions.read()?;
        let active_profile = document
            .active_profile_id
            .as_deref()
            .and_then(|id| catalog.profiles.iter().find(|profile| profile.id == id));
        let requirements = active_profile.map_or_else(AutoConfigRequirements::default, |profile| {
            profile_requirements(profile, &catalog.custom_nodes)
        });
        let registered = self
            .registry
            .auto_config_candidates(&self.target, &requirements)?;
        let mut checks = vec![AutoConfigCheck {
            id: "platform".into(),
            status: "pass".into(),
            detail: self.target.platform(),
        }];
        if self.target.os == "darwin" {
            let private_dns = requirements
                .required_features
                .contains(features::DNS_TUN_CAPTURE);
            checks.push(AutoConfigCheck {
                id: "dns-requirement".into(),
                status: if private_dns {
                    "pass".into()
                } else {
                    "info".into()
                },
                detail: if private_dns {
                    "active profile requires native macOS DNS integration".into()
                } else {
                    "active profile does not require private DNS integration".into()
                },
            });
        }
        match active_profile.filter(|profile| profile_has_inputs(profile)) {
            Some(profile) => checks.push(AutoConfigCheck {
                id: "active-profile".into(),
                status: "pass".into(),
                detail: profile.name.clone(),
            }),
            None => checks.push(AutoConfigCheck {
                id: "active-profile".into(),
                status: "warning".into(),
                detail: "configure a subscription before applying the recommendation".into(),
            }),
        }
        let candidates: Vec<_> = registered
            .into_iter()
            .map(|candidate| {
                let reference = CoreRef::parse(&candidate.reference)
                    .expect("registry validated recommendation reference");
                AutoConfigCandidate {
                    installed: reference_installed(&document, &reference),
                    selected: selection_matches(&document, &reference),
                    candidate,
                }
            })
            .collect();
        Ok(AutoConfigReport {
            checked_at: Utc::now(),
            platform: self.target.os.clone(),
            architecture: self.target.arch.clone(),
            policy_version: sempre_core::AUTO_CONFIG_POLICY_VERSION,
            requirements,
            recommendation: candidates
                .iter()
                .find(|candidate| candidate.candidate.eligible)
                .cloned(),
            candidates,
            checks,
        })
    }
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn apply_core_configuration(
        &self,
        candidate_id: &str,
    ) -> Result<AutoConfigApplyResult, ManagerError> {
        let report = self.diagnose_core_configuration()?;
        let selected_id = if candidate_id.is_empty() {
            report
                .recommendation
                .as_ref()
                .map(|candidate| candidate.candidate.id.as_str())
                .unwrap_or_default()
        } else {
            candidate_id
        };
        let recommendation = report
            .candidates
            .into_iter()
            .find(|candidate| candidate.candidate.id == selected_id)
            .ok_or_else(|| {
                ManagerError::InvalidOperation(format!(
                    "automatic configuration candidate {selected_id:?} is not available for this host"
                ))
            })?;
        if !recommendation.candidate.eligible {
            return Err(ManagerError::InvalidOperation(format!(
                "automatic configuration candidate {selected_id:?} is incompatible: {}",
                recommendation.candidate.blockers.join(", ")
            )));
        }
        let installed = self
            .install_core(&recommendation.candidate.reference)
            .await?;
        let install_change = CoreChange {
            changed: installed.changed,
            message: if installed.changed {
                format!("{}@{} installed", installed.core, installed.version)
            } else {
                format!(
                    "{}@{} is already installed",
                    installed.core, installed.version
                )
            },
            ..CoreChange::default()
        };
        let selected = self
            .select_core(&recommendation.candidate.reference)
            .await?;
        Ok(AutoConfigApplyResult {
            recommendation,
            changes: vec![install_change, selected],
        })
    }
}

fn profile_has_inputs(profile: &Profile) -> bool {
    profile
        .extra
        .get("mode")
        .and_then(serde_json::Value::as_str)
        == Some("remote")
        || (!profile.editor.servers.trim().is_empty() && profile.editor.servers.trim() != "[]")
        || !profile.custom_node_ids.is_empty()
        || profile.sources.iter().any(|source| source.enabled)
}

fn profile_requirements(profile: &Profile, custom_nodes: &[CustomNode]) -> AutoConfigRequirements {
    let mut requirements = AutoConfigRequirements::default();
    if profile
        .dns
        .pointer("/shared/systemDnsTakeoverEnabled")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        requirements.require_feature(features::DNS_TUN_CAPTURE);
    }
    match profile.transparent_proxy.mode.as_str() {
        "tun-router" => requirements.require_feature(features::TRANSPARENT_TUN),
        "tproxy" => requirements.require_feature(features::TRANSPARENT_TPROXY),
        "ebpf-router" => requirements.require_feature(features::TRANSPARENT_EBPF),
        _ => {}
    }
    if !matches!(
        profile.transparent_proxy.interface_mode.as_str(),
        "" | "all"
    ) {
        requirements.require_feature(features::TRANSPARENT_INTERFACES);
    }

    let private_access_enabled = profile
        .private_access
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    let private_connectors = if private_access_enabled {
        profile
            .private_access
            .get("connectors")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|connector| {
                connector
                    .get("enabled")
                    .and_then(serde_json::Value::as_bool)
                    != Some(false)
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if !private_connectors.is_empty() {
        requirements.require_feature(features::PRIVATE_ACCESS);
    }
    if private_connectors.iter().any(|connector| {
        connector
            .get("dns")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .any(|dns| {
                dns.get("server")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|server| !server.trim().is_empty())
            })
    }) {
        requirements.require_feature(features::DNS_TUN_CAPTURE);
    }

    let editor_servers = editor_servers(profile);
    for value in profile.manual_servers.iter().chain(editor_servers.iter()) {
        if let Some(protocol) = value.get("type").and_then(serde_json::Value::as_str) {
            requirements.require_protocol(normalize_protocol(protocol));
        }
    }
    for node in custom_nodes
        .iter()
        .filter(|node| profile.custom_node_ids.contains(&node.id))
    {
        if let Some(protocol) = node.proxy.get("type").and_then(serde_json::Value::as_str) {
            requirements.require_protocol(normalize_protocol(protocol));
        }
    }
    requirements
}

fn editor_servers(profile: &Profile) -> Vec<serde_json::Value> {
    serde_json::from_str(&profile.editor.servers).unwrap_or_default()
}

fn normalize_protocol(protocol: &str) -> &str {
    match protocol {
        "ss" => "shadowsocks",
        value => value,
    }
}

fn reference_installed(document: &Document, reference: &CoreRef) -> bool {
    let Some(source) = document.cores.get(&reference.core).and_then(|core| {
        reference.repository.as_deref().map_or_else(
            || Some(&core.default),
            |repository| core.custom.get(repository),
        )
    }) else {
        return false;
    };
    let version = if reference.is_channel() {
        source.channels.get(&reference.reference)
    } else {
        Some(&reference.reference)
    };
    version.is_some_and(|version| source.installed.contains_key(version))
}

fn selection_matches(document: &Document, reference: &CoreRef) -> bool {
    document.selected.as_ref().is_some_and(|selection| {
        selection.core == reference.core
            && selection.repository == reference.repository
            && selection.reference == reference.reference
    })
}

#[cfg(test)]
mod tests {
    use sempre_state::{Layout, Store};
    use serde_json::json;

    use super::*;

    #[test]
    fn system_dns_takeover_requires_tun_capture() {
        let profile: Profile = serde_json::from_value(json!({
            "dns": { "shared": { "systemDnsTakeoverEnabled": true } }
        }))
        .expect("profile");
        let requirements = profile_requirements(&profile, &[]);
        assert!(
            requirements
                .required_features
                .contains(features::DNS_TUN_CAPTURE)
        );
    }

    #[tokio::test]
    async fn diagnosis_is_local_and_apply_rejects_unknown_candidates_before_download() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = Manager::new(Store::new(Layout::at(root.path()))).expect("manager");
        let report = manager.diagnose_core_configuration().expect("diagnosis");
        assert!(!report.candidates.is_empty());
        if manager.target.os == "darwin" {
            let recommendation = &report
                .recommendation
                .as_ref()
                .expect("recommendation")
                .candidate;
            assert_eq!(recommendation.reference, "sing-box@stable");
            assert_eq!(recommendation.score, Some(93));
        }
        assert_eq!(
            report.checks.last().expect("profile check").status,
            "warning"
        );
        let error = manager
            .apply_core_configuration("unavailable")
            .await
            .expect_err("unknown recommendation must fail");
        assert!(error.to_string().contains("is not available for this host"));
    }

    #[tokio::test]
    async fn private_dns_profile_requires_the_native_dns_candidate() {
        let root = tempfile::tempdir().expect("temporary directory");
        let mut manager = Manager::new(Store::new(Layout::at(root.path()))).expect("manager");
        manager.target = sempre_core::Target {
            os: "darwin".into(),
            arch: "arm64".into(),
            amd64_level: 0,
        };
        let profile_id = manager.subscriptions.read().expect("catalog").profiles[0]
            .id
            .clone();
        manager
            .subscriptions
            .update(|catalog| {
                let profile = &mut catalog.profiles[0];
                profile.editor.servers = "[{}]".into();
                profile.private_access = json!({
                    "enabled": true,
                    "connectors": [{
                        "type": "wireguard",
                        "endpoint": { "private_key": "test", "peers": [] },
                        "dns": [{ "server": "10.8.28.1", "domainSuffixes": ["example.test"] }]
                    }]
                });
                Ok(())
            })
            .expect("seed profile");
        manager
            .store
            .update(|document| {
                document.active_profile_id = Some(profile_id);
                Ok(())
            })
            .expect("activate profile");

        let report = manager.diagnose_core_configuration().expect("diagnosis");
        assert_eq!(
            report.policy_version,
            sempre_core::AUTO_CONFIG_POLICY_VERSION
        );
        assert!(
            report
                .requirements
                .required_features
                .contains(features::PRIVATE_ACCESS)
        );
        assert!(
            report
                .requirements
                .required_features
                .contains(features::DNS_TUN_CAPTURE)
        );
        let recommendation = report.recommendation.expect("recommendation");
        assert_eq!(recommendation.candidate.id, "sing-box/macos-native-dns-v14");
        assert!(recommendation.candidate.eligible);
        assert_eq!(recommendation.candidate.score, Some(70));
        assert!(
            report
                .candidates
                .iter()
                .filter(|candidate| candidate.candidate.id != recommendation.candidate.id)
                .all(|candidate| !candidate.candidate.eligible)
        );
        let error = manager
            .apply_core_configuration("sing-box/macos-standalone-v12")
            .await
            .expect_err("ineligible candidate");
        assert!(error.to_string().contains("is incompatible"));
    }

    #[test]
    fn requirements_cover_runtime_mode_interfaces_and_known_protocols() {
        let mut profile = Profile::default();
        profile.transparent_proxy.mode = "tproxy".into();
        profile.transparent_proxy.interface_mode = "include".into();
        profile.editor.servers = json!([
            { "name": "one", "type": "ss", "server": "127.0.0.1", "port": 1 },
            { "name": "two", "type": "vless", "server": "127.0.0.1", "port": 2 }
        ])
        .to_string();

        let requirements = profile_requirements(&profile, &[]);
        assert_eq!(
            requirements.required_features,
            [
                features::TRANSPARENT_INTERFACES,
                features::TRANSPARENT_TPROXY
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        );
        assert_eq!(
            requirements.required_protocols,
            ["shadowsocks".to_owned(), "vless".to_owned()]
                .into_iter()
                .collect()
        );
    }
}
