use sempre_converter::{CustomNode, Profile, SourceSnapshot, Target, prepare_profile};
use sempre_subscription::SubscriptionError;
use serde_json::Value;

use crate::{Manager, ManagerError, VersionRunner, subscription::validate_source_content};

pub(crate) struct LoadedSources {
    pub(crate) snapshots: Vec<SourceSnapshot>,
    pub(crate) warnings: Vec<String>,
    pub(crate) failed_ids: Vec<String>,
}

impl LoadedSources {
    pub(crate) fn for_compile(
        &self,
        profile: &mut Profile,
        target: &Target,
        custom_nodes: &[CustomNode],
    ) -> Result<(), ManagerError> {
        if self.failed_ids.is_empty() {
            return Ok(());
        }
        let effective = prepare_profile(profile, target)?;
        let has_custom = effective
            .custom_node_ids
            .iter()
            .any(|id| custom_nodes.iter().any(|node| node.id == *id));
        if self.snapshots.is_empty() && effective.manual_servers.is_empty() && !has_custom {
            return Err(SubscriptionError::Fetch(format!(
                "all enabled sources failed and no manual or selected custom nodes are available: {}",
                self.warnings.join("; ")
            ))
            .into());
        }
        for source in &mut profile.sources {
            if self.failed_ids.contains(&source.id) {
                source.enabled = false;
            }
        }
        Ok(())
    }
}

impl<R: VersionRunner> Manager<R> {
    pub(crate) async fn load_profile_sources(
        &self,
        profile: &mut Profile,
        force: bool,
        cached_only: bool,
        persist: bool,
    ) -> Result<LoadedSources, ManagerError> {
        let mut loaded = LoadedSources {
            snapshots: Vec::new(),
            warnings: Vec::new(),
            failed_ids: Vec::new(),
        };
        for source in profile.sources.iter_mut().filter(|source| source.enabled) {
            let result = if cached_only {
                self.fetcher
                    .load_cached(source.clone(), validate_source_content)
            } else if persist {
                self.fetcher
                    .load(source.clone(), force, validate_source_content)
                    .await
            } else {
                self.fetcher
                    .inspect(source.clone(), force, validate_source_content)
                    .await
            };
            match result {
                Ok(result) => {
                    if result.from_cache
                        && result.source.extra.get("last_status")
                            == Some(&Value::String("last-known-good cache".into()))
                    {
                        let reason = result
                            .source
                            .extra
                            .get("last_error")
                            .and_then(Value::as_str)
                            .unwrap_or("refresh failed");
                        loaded.warnings.push(format!(
                            "source {:?} used its last-known-good snapshot: {reason}",
                            result.source.id
                        ));
                    }
                    *source = result.source;
                    loaded.snapshots.push(result.snapshot);
                }
                Err(error) => {
                    source
                        .extra
                        .insert("last_status".into(), Value::String("failed".into()));
                    source
                        .extra
                        .insert("last_error".into(), Value::String(error.to_string()));
                    let label = if source.remark.is_empty() {
                        &source.id
                    } else {
                        &source.remark
                    };
                    loaded
                        .warnings
                        .push(format!("source {label:?} failed: {error}"));
                    loaded.failed_ids.push(source.id.clone());
                }
            }
        }
        Ok(loaded)
    }
}
