use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use sempre_converter::{
    CompileRequest, CompileResult, Diagnostic, FieldDiff, Profile, SourceSnapshot, Target, compile,
    parse_subscription,
};
use sempre_state::{ConfigBuild, Document};
use sempre_subscription::{Catalog, SubscriptionError};
use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{CoreChange, Manager, ManagerError, ValidationRunner, VersionRunner};

#[derive(Clone, Debug, Serialize)]
pub struct SubscriptionRender {
    pub format: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub platform: String,
    pub content: String,
    pub artifact_hash: String,
    pub node_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub field_diffs: Vec<FieldDiff>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub node_origins: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub runtime_validated: bool,
}

pub(crate) struct RenderedProfile {
    pub(crate) render: SubscriptionRender,
    pub(crate) updated: Profile,
    pub(crate) target: Target,
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn import_subscription_source(
        &self,
        remark: &str,
        content: &str,
    ) -> Result<CoreChange, ManagerError> {
        if content.len() > sempre_subscription::MAX_SOURCE_SIZE {
            return Err(SubscriptionError::SourceTooLarge {
                limit: sempre_subscription::MAX_SOURCE_SIZE,
            }
            .into());
        }
        let document = self.store.read()?;
        let catalog = self.subscriptions.read()?;
        let profile = document
            .active_profile_id
            .as_deref()
            .and_then(|id| catalog.profiles.iter().find(|profile| profile.id == id))
            .or_else(|| catalog.profiles.first())
            .ok_or_else(|| SubscriptionError::Invalid("no subscription profile exists".into()))?;
        if profile_mode(profile) == "remote" {
            return Err(SubscriptionError::Invalid(
                "remote profiles are read-only; edit the profile on its Sempre server".into(),
            )
            .into());
        }
        let profile_id = profile.id.clone();
        let source = sempre_converter::Source {
            id: Uuid::new_v4().to_string(),
            kind: "raw".into(),
            enabled: true,
            url: String::new(),
            remark: remark.trim().into(),
            prefix: String::new(),
            content: content.into(),
            user_agent: String::new(),
            extra: Map::new(),
        };
        self.subscriptions.update(|catalog| {
            let profile = catalog
                .profiles
                .iter_mut()
                .find(|profile| profile.id == profile_id)
                .ok_or_else(|| SubscriptionError::Invalid("profile was not found".into()))?;
            profile.sources.push(source);
            profile.revision += 1;
            Ok(())
        })?;
        let (change, _) = self.activate_subscription_profile(&profile_id).await?;
        Ok(change)
    }

    pub(crate) async fn recompile_subscription_profile(
        &self,
        id: &str,
    ) -> Result<(CoreChange, SubscriptionRender), ManagerError> {
        let _operation = self.store.acquire_operation()?;
        self.prepare_subscription_locked(id, false, false).await
    }

    pub fn clear_subscription_cache(&self) -> Result<CoreChange, ManagerError> {
        let _operation = self.store.acquire_operation()?;
        self.subscriptions.clear_cache()?;
        Ok(CoreChange {
            changed: true,
            message: "subscription fetch cache cleared".into(),
            ..CoreChange::default()
        })
    }

    pub async fn refresh_subscription_profile(
        &self,
        id: &str,
    ) -> Result<(CoreChange, SubscriptionRender), ManagerError> {
        let _operation = self.store.acquire_operation()?;
        self.prepare_subscription_locked(id, false, true).await
    }

    pub async fn activate_subscription_profile(
        &self,
        id: &str,
    ) -> Result<(CoreChange, SubscriptionRender), ManagerError> {
        let _operation = self.store.acquire_operation()?;
        self.prepare_subscription_locked(id, true, true).await
    }

    async fn prepare_subscription_locked(
        &self,
        id: &str,
        activate: bool,
        refresh: bool,
    ) -> Result<(CoreChange, SubscriptionRender), ManagerError> {
        let catalog = self.subscriptions.read()?;
        let profile = find_profile(&catalog, id)?.clone();
        let document = self.store.read()?;
        let mut rendered = self
            .render_subscription(&catalog, &profile, &document, refresh)
            .await?;
        let now = Utc::now();
        let active = activate || document.active_profile_id.as_deref() == Some(id);
        let profile_changed = activate && document.active_profile_id.as_deref() != Some(id);
        let build = config_build(&rendered.updated, &rendered.target)?;
        let mut change = if active {
            let profile_id = id.to_owned();
            self.activate_config_content_updating(
                rendered.render.content.as_bytes(),
                build,
                move |state, changed| {
                    if activate {
                        state.active_profile_id = Some(profile_id);
                    }
                    state.subscription.last_check = Some(now);
                    state.subscription.last_result = Some(if changed {
                        state.subscription.last_change = Some(now);
                        "configuration updated".into()
                    } else {
                        "no change".into()
                    });
                },
            )
            .await?
        } else {
            self.validate_config_content(rendered.render.content.as_bytes())
                .await?;
            self.subscriptions
                .save_blob(rendered.render.content.as_bytes())?;
            CoreChange {
                changed: true,
                message: "subscription profile refreshed, compiled, and validated".into(),
                ..CoreChange::default()
            }
        };
        change.changed |= profile_changed;
        rendered.render.runtime_validated = true;
        record_compilation(
            &self.subscriptions,
            &profile,
            rendered.updated,
            &rendered.render,
            now,
        )?;
        if active {
            change.message = "subscription profile refreshed, validated, and staged".into();
        }
        Ok((change, rendered.render))
    }

    pub(crate) async fn prepare_active_subscription_for_runtime_locked(
        &self,
    ) -> Result<(), ManagerError> {
        let document = self.store.read()?;
        let Some(id) = document.active_profile_id.as_deref() else {
            return Ok(());
        };
        let catalog = self.subscriptions.read()?;
        let profile = find_profile(&catalog, id)?;
        let (target, _) = self.subscription_target(&document)?;
        let expected = config_build(profile, &target)?;
        if document
            .selected
            .as_ref()
            .and_then(|selected| document.config_builds.get(&selected.core))
            == Some(&expected)
        {
            return Ok(());
        }
        self.prepare_subscription_locked(id, false, false).await?;
        Ok(())
    }

    pub(crate) fn active_profile_config_pending(&self, document: &Document) -> bool {
        let Some(id) = document.active_profile_id.as_deref() else {
            return false;
        };
        let Some(selected) = document.selected.as_ref() else {
            return false;
        };
        let Ok(catalog) = self.subscriptions.read() else {
            return false;
        };
        let Ok(profile) = find_profile(&catalog, id) else {
            return false;
        };
        let Ok((target, _)) = self.subscription_target(document) else {
            return false;
        };
        config_build(profile, &target)
            .is_ok_and(|expected| document.config_builds.get(&selected.core) != Some(&expected))
    }

    async fn render_subscription(
        &self,
        catalog: &Catalog,
        profile: &Profile,
        document: &Document,
        refresh: bool,
    ) -> Result<RenderedProfile, ManagerError> {
        let (target, mut adapter_warnings) = self.subscription_target(document)?;
        self.render_subscription_for_target(
            catalog,
            profile,
            target,
            &mut adapter_warnings,
            refresh,
        )
        .await
    }

    pub(crate) async fn render_subscription_for_target(
        &self,
        catalog: &Catalog,
        profile: &Profile,
        target: Target,
        adapter_warnings: &mut Vec<String>,
        refresh: bool,
    ) -> Result<RenderedProfile, ManagerError> {
        if profile_mode(profile) == "remote" {
            let remote = self.remote.render(profile, &target).await?;
            validate_runtime_profile(&remote.profile)?;
            let warnings = adapter_warnings.drain(..).chain(remote.warnings).collect();
            return Ok(RenderedProfile {
                render: SubscriptionRender {
                    format: remote.target.format,
                    version: remote.target.version,
                    platform: remote.target.platform,
                    content: remote.content,
                    artifact_hash: remote.artifact_hash,
                    node_count: remote.node_count,
                    field_diffs: Vec::new(),
                    node_origins: BTreeMap::new(),
                    diagnostics: Vec::new(),
                    warnings,
                    runtime_validated: false,
                },
                updated: remote.profile,
                target,
            });
        }

        validate_runtime_profile(profile)?;
        let mut updated = profile.clone();
        let mut snapshots = Vec::<SourceSnapshot>::new();
        for source in updated.sources.iter_mut().filter(|source| source.enabled) {
            let result = if refresh {
                self.fetcher
                    .load(source.clone(), true, validate_source_content)
                    .await?
            } else {
                self.fetcher
                    .load_cached(source.clone(), validate_source_content)?
            };
            *source = result.source;
            snapshots.push(result.snapshot);
        }
        let (provider_snapshots, provider_warnings) = self
            .load_rule_provider_snapshots(&updated, &target, refresh)
            .await?;
        snapshots.extend(provider_snapshots);
        adapter_warnings.extend(provider_warnings);
        let compiled = compile(&CompileRequest {
            protocol: 1,
            profile: updated.clone(),
            snapshots,
            custom_nodes: catalog.custom_nodes.clone(),
            target: target.clone(),
        })?;
        Ok(RenderedProfile {
            render: local_render(compiled, std::mem::take(adapter_warnings)),
            updated,
            target,
        })
    }

    fn subscription_target(
        &self,
        document: &Document,
    ) -> Result<(Target, Vec<String>), ManagerError> {
        if document.selected.is_none() {
            return Err(ManagerError::NoSelectedCore);
        }
        let (reference, version) = crate::config::configuration_target(document)?;
        self.subscription_target_for(&reference, &version)
    }

    pub(crate) fn subscription_target_for(
        &self,
        reference: &sempre_core::CoreRef,
        version: &str,
    ) -> Result<(Target, Vec<String>), ManagerError> {
        let configuration = self.configuration_target_for(reference, version)?;
        let compiler = configuration.compiler_target;
        let mut target = Target::parse(&compiler.format)?;
        target.core = configuration.core;
        target.platform = compiler.platform;
        if let Some(version) = compiler.version {
            target.version = version;
        }
        Ok((target, compiler.warnings))
    }
}

pub(crate) fn local_render(result: CompileResult, mut warnings: Vec<String>) -> SubscriptionRender {
    warnings.extend(result.diagnostics.iter().map(|item| item.message.clone()));
    SubscriptionRender {
        format: result.format,
        version: result.version,
        platform: result.platform,
        content: result.content,
        artifact_hash: result.artifact_hash,
        node_count: result.node_count,
        field_diffs: result.field_diffs,
        node_origins: result.node_origins.into_iter().collect(),
        diagnostics: result.diagnostics,
        warnings,
        runtime_validated: false,
    }
}

pub(crate) fn validate_source_content(content: &str) -> Result<(), SubscriptionError> {
    let parsed = parse_subscription(content);
    if parsed.nodes.is_empty() {
        let detail = if parsed.diagnostics.is_empty() {
            "no supported nodes were found".into()
        } else {
            parsed.diagnostics.join("; ")
        };
        return Err(SubscriptionError::Invalid(detail));
    }
    Ok(())
}

pub(crate) fn find_profile<'a>(
    catalog: &'a Catalog,
    id: &str,
) -> Result<&'a Profile, ManagerError> {
    catalog
        .profiles
        .iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| ManagerError::ProfileNotFound(id.into()))
}

pub(crate) fn profile_mode(profile: &Profile) -> &str {
    profile
        .extra
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("local")
}

fn validate_runtime_profile(profile: &Profile) -> Result<(), ManagerError> {
    if profile.transparent_proxy.mode != "tun-router" {
        return Ok(());
    }
    let name = profile.transparent_proxy.tun.interface_name.trim();
    if name.is_empty() || name.len() > 15 {
        return Err(SubscriptionError::Invalid(
            "TUN interface name must contain 1 to 15 characters".into(),
        )
        .into());
    }
    Ok(())
}

pub(crate) fn config_build(
    profile: &Profile,
    target: &Target,
) -> Result<ConfigBuild, ManagerError> {
    Ok(ConfigBuild {
        profile_id: profile.id.clone(),
        profile_revision: profile.revision,
        target_key: format!("{}|{}|{}", target.format, target.version, target.platform),
        runtime_key: Some(runtime_key(profile)?),
    })
}

fn runtime_key(profile: &Profile) -> Result<String, ManagerError> {
    let value = json!({
        "transparent_proxy": profile.transparent_proxy,
        "local_proxy": profile.local_proxy,
        "management_api": profile.management_api,
    });
    let data = serde_json::to_vec(&canonical(value)).map_err(|error| {
        SubscriptionError::Invalid(format!("encode profile runtime settings: {error}"))
    })?;
    Ok(format!("{:x}", Sha256::digest(data)))
}

fn canonical(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
        Value::Object(values) => {
            let sorted: BTreeMap<_, _> = values.into_iter().collect();
            Value::Object(
                sorted
                    .into_iter()
                    .map(|(key, value)| (key, canonical(value)))
                    .collect::<Map<_, _>>(),
            )
        }
        value => value,
    }
}

pub(crate) fn record_compilation(
    store: &sempre_subscription::SubscriptionStore,
    original: &Profile,
    updated: Profile,
    render: &SubscriptionRender,
    now: DateTime<Utc>,
) -> Result<(), ManagerError> {
    store.update(|catalog| {
        let item = catalog
            .profiles
            .iter_mut()
            .find(|item| item.id == original.id)
            .ok_or_else(|| SubscriptionError::Invalid("profile was not found".into()))?;
        if item.revision != original.revision {
            return Err(SubscriptionError::Invalid(
                "subscription profile changed while compiling; retry the command".into(),
            ));
        }
        item.sources = updated.sources;
        if profile_mode(original) == "remote" {
            item.local_proxy = updated.local_proxy;
            item.transparent_proxy = updated.transparent_proxy;
            item.management_api = updated.management_api;
            if let Some(remote) = updated.extra.get("remote") {
                item.extra.insert("remote".into(), remote.clone());
            }
        }
        item.extra.insert("last_check".into(), json!(now));
        item.extra.insert(
            "last_result".into(),
            json!("configuration compiled and runtime validated"),
        );
        if item.extra.get("last_config_hash") != Some(&json!(render.artifact_hash)) {
            item.extra.insert("last_change".into(), json!(now));
        }
        item.extra
            .insert("last_config_hash".into(), json!(render.artifact_hash));
        item.extra
            .insert("last_runtime_validated".into(), json!(true));
        item.extra
            .insert("last_compiler_target".into(), json!(render.format));
        item.extra
            .insert("last_compiler_warnings".into(), json!(render.warnings));
        Ok(())
    })?;
    Ok(())
}

#[cfg(test)]
mod tests;
