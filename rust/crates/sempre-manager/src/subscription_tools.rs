use std::collections::BTreeMap;

use sempre_converter::{
    CompileRequest, FieldDiff, ParseResult, PreviewNode, Profile, Source, Target, compile,
    parse_subscription, preview_nodes, trace_node_steps,
};
use sempre_subscription::FetchObservation;
use serde::Serialize;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    Manager, ManagerError, SubscriptionRender, ValidationRunner, VersionRunner,
    subscription::{find_profile, local_render, profile_mode, validate_source_content},
};

#[derive(Clone, Debug, Serialize)]
pub struct SourceTestResult {
    pub source: Source,
    pub parse: ParseResult,
    pub from_cache: bool,
    pub content_hash: String,
    pub bytes: usize,
    pub observation: FetchObservation,
    #[serde(skip)]
    pub raw_text: String,
}

pub struct SourceInspection {
    pub result: Result<SourceTestResult, ManagerError>,
    pub observation: FetchObservation,
}

#[derive(Clone, Debug)]
pub struct ProfileDebugSource {
    pub source_index: usize,
    pub source: Source,
    pub parse: ParseResult,
    pub raw_text: String,
    pub from_cache: bool,
    pub observation: FetchObservation,
}

pub enum ProfileDebugProgress {
    Configured {
        profile: Box<Profile>,
        effective: Box<Profile>,
    },
    SourceStarted {
        source_index: usize,
        source: Box<Source>,
    },
    SourceFetched(Box<ProfileDebugSource>),
    SourceFailed {
        source_index: usize,
        source: Box<Source>,
        error: String,
        observation: Box<FetchObservation>,
    },
    RulesStarted,
    RulesFinished {
        providers: Vec<sempre_converter::RuleProvider>,
        snapshot_ids: Vec<String>,
        warnings: Vec<String>,
    },
    Compiling,
}

#[derive(Clone, Debug)]
pub struct ProfileDebugResult {
    pub profile: Profile,
    pub effective: Profile,
    pub sources: Vec<ProfileDebugSource>,
    pub nodes: Vec<PreviewNode>,
    pub render: SubscriptionRender,
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn debug_subscription_profile(
        &self,
        id: &str,
        format: &str,
        progress: mpsc::Sender<ProfileDebugProgress>,
    ) -> Result<ProfileDebugResult, ManagerError> {
        let catalog = self.subscriptions.read()?;
        let profile = find_profile(&catalog, id)?.clone();
        if profile_mode(&profile) == "remote" {
            return Err(sempre_subscription::SubscriptionError::Invalid(
                "remote profiles expose compiled artifacts, not source diagnostics".into(),
            )
            .into());
        }
        let target = Target::parse(format)?;
        let mut updated = profile.clone();
        let effective = sempre_converter::prepare_profile(&profile, &target)?;
        let _ = progress
            .send(ProfileDebugProgress::Configured {
                profile: Box::new(profile.clone()),
                effective: Box::new(effective.clone()),
            })
            .await;
        let (loaded, sources) = self.inspect_debug_sources(&mut updated, &progress).await;
        let mut snapshots = loaded.snapshots.clone();
        let mut warnings = loaded.warnings.clone();
        loaded.for_compile(&mut updated, &target, &catalog.custom_nodes)?;
        let _ = progress.send(ProfileDebugProgress::RulesStarted).await;
        let (provider_snapshots, provider_warnings) = self
            .load_rule_provider_snapshots(
                &updated,
                &target,
                true,
                false,
                crate::rule_provider::RuleSnapshotPolicy::RequireSnapshot,
            )
            .await?;
        let _ = progress
            .send(ProfileDebugProgress::RulesFinished {
                providers: effective.rule_providers.clone(),
                snapshot_ids: provider_snapshots
                    .iter()
                    .map(|snapshot| snapshot.source_id.clone())
                    .collect(),
                warnings: provider_warnings.clone(),
            })
            .await;
        snapshots.extend(provider_snapshots);
        warnings.extend(provider_warnings);
        let request = CompileRequest {
            protocol: 1,
            profile: updated.clone(),
            snapshots,
            custom_nodes: catalog.custom_nodes,
            target: target.clone(),
        };
        let nodes = preview_nodes(&request)?;
        let _ = progress.send(ProfileDebugProgress::Compiling).await;
        let compiled = compile(&request)?;
        if !loaded.failed_ids.is_empty() && compiled.node_count == 0 {
            return Err(sempre_subscription::SubscriptionError::Fetch(format!(
                "enabled sources failed and no usable nodes remain: {}",
                loaded.warnings.join("; ")
            ))
            .into());
        }
        let render = local_render(compiled, warnings);
        Ok(ProfileDebugResult {
            profile,
            effective,
            sources,
            nodes,
            render,
        })
    }

    async fn inspect_debug_sources(
        &self,
        profile: &mut Profile,
        progress: &mpsc::Sender<ProfileDebugProgress>,
    ) -> (
        crate::source_loading::LoadedSources,
        Vec<ProfileDebugSource>,
    ) {
        let mut loaded = crate::source_loading::LoadedSources {
            snapshots: Vec::new(),
            warnings: Vec::new(),
            failed_ids: Vec::new(),
        };
        let mut sources = Vec::new();
        for (index, source) in profile.sources.iter_mut().enumerate() {
            if !source.enabled {
                continue;
            }
            let _ = progress
                .send(ProfileDebugProgress::SourceStarted {
                    source_index: index + 1,
                    source: Box::new(source.clone()),
                })
                .await;
            let inspection = self
                .fetcher
                .inspect_observed(source.clone(), true, validate_source_content)
                .await;
            match inspection.result {
                Ok(result) => {
                    if result.from_cache
                        && result.source.extra.get("last_status")
                            == Some(&serde_json::Value::String("last-known-good cache".into()))
                    {
                        let reason = result
                            .source
                            .extra
                            .get("last_error")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("refresh failed");
                        loaded.warnings.push(format!(
                            "source {:?} used its last-known-good snapshot: {reason}",
                            result.source.id
                        ));
                    }
                    *source = result.source.clone();
                    let parsed = ProfileDebugSource {
                        source_index: index + 1,
                        source: result.source,
                        parse: parse_subscription(&result.snapshot.content),
                        raw_text: result.snapshot.content.clone(),
                        from_cache: result.from_cache,
                        observation: inspection.observation,
                    };
                    let _ = progress
                        .send(ProfileDebugProgress::SourceFetched(Box::new(
                            parsed.clone(),
                        )))
                        .await;
                    sources.push(parsed);
                    loaded.snapshots.push(result.snapshot);
                }
                Err(error) => {
                    let message = error.to_string();
                    loaded
                        .warnings
                        .push(format!("source {:?} failed: {message}", source.id));
                    loaded.failed_ids.push(source.id.clone());
                    let _ = progress
                        .send(ProfileDebugProgress::SourceFailed {
                            source_index: index + 1,
                            source: Box::new(source.clone()),
                            error: message,
                            observation: Box::new(inspection.observation),
                        })
                        .await;
                }
            }
        }
        (loaded, sources)
    }

    pub async fn render_subscription_profile(
        &self,
        id: &str,
        format: &str,
        force: bool,
    ) -> Result<SubscriptionRender, ManagerError> {
        let catalog = self.subscriptions.read()?;
        let profile = find_profile(&catalog, id)?.clone();
        let target = Target::parse(format)?;
        if profile_mode(&profile) == "remote" {
            let remote = self.remote.render(&profile, &target).await?;
            return Ok(SubscriptionRender {
                format: remote.target.format,
                version: remote.target.version,
                platform: remote.target.platform,
                content: remote.content,
                artifact_hash: remote.artifact_hash,
                node_count: remote.node_count,
                field_diffs: Vec::new(),
                node_origins: BTreeMap::default(),
                diagnostics: Vec::new(),
                warnings: remote.warnings,
                runtime_validated: false,
            });
        }
        let (mut request, source_warnings) = self
            .load_profile_request(profile, catalog.custom_nodes, target, force)
            .await?;
        let (provider_snapshots, provider_warnings) = self
            .load_rule_provider_snapshots(
                &request.profile,
                &request.target,
                force,
                false,
                crate::rule_provider::RuleSnapshotPolicy::RequireSnapshot,
            )
            .await?;
        request.snapshots.extend(provider_snapshots);
        let compiled = compile(&request)?;
        if !source_warnings.is_empty() && compiled.node_count == 0 {
            return Err(sempre_subscription::SubscriptionError::Fetch(format!(
                "enabled sources failed and no usable nodes remain: {}",
                source_warnings.join("; ")
            ))
            .into());
        }
        Ok(local_render(
            compiled,
            source_warnings
                .into_iter()
                .chain(provider_warnings)
                .collect(),
        ))
    }

    pub async fn preview_subscription_nodes(
        &self,
        id: &str,
        format: &str,
    ) -> Result<Vec<PreviewNode>, ManagerError> {
        let catalog = self.subscriptions.read()?;
        let profile = find_profile(&catalog, id)?.clone();
        if profile_mode(&profile) == "remote" {
            return Err(sempre_subscription::SubscriptionError::Invalid(
                "remote profiles expose compiled artifacts, not editable source nodes".into(),
            )
            .into());
        }
        let (request, _) = self
            .load_profile_request(profile, catalog.custom_nodes, Target::parse(format)?, true)
            .await?;
        Ok(preview_nodes(&request)?)
    }

    pub async fn trace_subscription_node(
        &self,
        id: &str,
        name: &str,
        format: &str,
    ) -> Result<FieldDiff, ManagerError> {
        let catalog = self.subscriptions.read()?;
        let profile = find_profile(&catalog, id)?.clone();
        if profile_mode(&profile) == "remote" {
            return Err(sempre_subscription::SubscriptionError::Invalid(
                "remote profiles do not expose editable node traces".into(),
            )
            .into());
        }
        let (request, _) = self
            .load_profile_request(profile, catalog.custom_nodes, Target::parse(format)?, true)
            .await?;
        compile(&request)?
            .field_diffs
            .into_iter()
            .find(|item| item.node == name)
            .ok_or_else(|| {
                sempre_subscription::SubscriptionError::Invalid(format!(
                    "node {name:?} was not found in conversion diagnostics"
                ))
                .into()
            })
    }

    pub async fn trace_subscription_node_steps(
        &self,
        id: &str,
        name: &str,
        format: &str,
    ) -> Result<serde_json::Value, ManagerError> {
        let catalog = self.subscriptions.read()?;
        let profile = find_profile(&catalog, id)?.clone();
        if profile_mode(&profile) == "remote" {
            return Err(sempre_subscription::SubscriptionError::Invalid(
                "remote profiles do not expose editable node traces".into(),
            )
            .into());
        }
        let (request, _) = self
            .load_profile_request(profile, catalog.custom_nodes, Target::parse(format)?, true)
            .await?;
        Ok(trace_node_steps(&request, name)?)
    }

    pub async fn test_subscription_source(
        &self,
        source: Source,
        force: bool,
    ) -> Result<SourceTestResult, ManagerError> {
        self.inspect_subscription_source(source, force).await.result
    }

    pub async fn inspect_subscription_source(
        &self,
        mut source: Source,
        force: bool,
    ) -> SourceInspection {
        if source.id.is_empty() {
            source.id = Uuid::new_v4().to_string();
        }
        let inspection = self
            .fetcher
            .inspect_observed(source, force, validate_source_content)
            .await;
        let observation = inspection.observation;
        let result = inspection.result.map(|result| SourceTestResult {
            parse: parse_subscription(&result.snapshot.content),
            raw_text: result.snapshot.content.clone(),
            source: result.source,
            from_cache: result.from_cache,
            content_hash: result.snapshot.content_hash,
            bytes: result.bytes,
            observation: observation.clone(),
        });
        SourceInspection {
            result: result.map_err(Into::into),
            observation,
        }
    }

    async fn load_profile_request(
        &self,
        mut profile: Profile,
        custom_nodes: Vec<sempre_converter::CustomNode>,
        target: Target,
        force: bool,
    ) -> Result<(CompileRequest, Vec<String>), ManagerError> {
        let loaded = self
            .load_profile_sources(&mut profile, force, false, false)
            .await?;
        loaded.for_compile(&mut profile, &target, &custom_nodes)?;
        Ok((
            CompileRequest {
                protocol: 1,
                profile,
                snapshots: loaded.snapshots,
                custom_nodes,
                target,
            },
            loaded.warnings,
        ))
    }
}
