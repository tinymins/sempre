use chrono::{DateTime, Utc};
use sempre_converter::Profile;
use sempre_core::CoreRef;
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
        let registered = self.registry.auto_config_candidates(&self.target)?;
        let mut checks = vec![AutoConfigCheck {
            id: "platform".into(),
            status: "pass".into(),
            detail: self.target.platform(),
        }];
        if self.target.os == "darwin" {
            checks.push(AutoConfigCheck {
                id: "system-dns-boundary".into(),
                status: "info".into(),
                detail: "Sempre does not modify macOS system DNS".into(),
            });
        }
        let catalog = self.subscriptions.read()?;
        match document
            .active_profile_id
            .as_deref()
            .and_then(|id| catalog.profiles.iter().find(|profile| profile.id == id))
            .filter(|profile| profile_has_inputs(profile))
        {
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
            recommendation: candidates.first().cloned(),
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

    use super::*;

    #[tokio::test]
    async fn diagnosis_is_local_and_apply_rejects_unknown_candidates_before_download() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = Manager::new(Store::new(Layout::at(root.path()))).expect("manager");
        let report = manager.diagnose_core_configuration().expect("diagnosis");
        assert!(!report.candidates.is_empty());
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
}
