use chrono::{DateTime, Utc};
use sempre_converter::Profile;
use sempre_core::{AutoConfigRequirements, CoreRef};
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
        let requirements = AutoConfigRequirements {
            private_dns: active_profile.is_some_and(profile_requires_private_dns),
        };
        let registered = self
            .registry
            .auto_config_candidates(&self.target, requirements)?;
        let mut checks = vec![AutoConfigCheck {
            id: "platform".into(),
            status: "pass".into(),
            detail: self.target.platform(),
        }];
        if self.target.os == "darwin" {
            checks.push(AutoConfigCheck {
                id: "dns-requirement".into(),
                status: if requirements.private_dns {
                    "pass".into()
                } else {
                    "info".into()
                },
                detail: if requirements.private_dns {
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

fn profile_requires_private_dns(profile: &Profile) -> bool {
    profile
        .private_access
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
        && profile
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
            .any(|connector| {
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
            })
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

    #[tokio::test]
    async fn diagnosis_is_local_and_apply_rejects_unknown_candidates_before_download() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = Manager::new(Store::new(Layout::at(root.path()))).expect("manager");
        let report = manager.diagnose_core_configuration().expect("diagnosis");
        assert!(!report.candidates.is_empty());
        if manager.target.os == "darwin" {
            assert_eq!(
                report
                    .recommendation
                    .as_ref()
                    .expect("recommendation")
                    .candidate
                    .reference,
                "sing-box@stable"
            );
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
        let recommendation = report.recommendation.expect("recommendation");
        assert_eq!(recommendation.candidate.id, "sing-box/macos-native-dns-v14");
        assert!(recommendation.candidate.eligible);
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
}
