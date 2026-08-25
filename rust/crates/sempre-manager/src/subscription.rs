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

struct RenderedProfile {
    render: SubscriptionRender,
    updated: Profile,
    target: Target,
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
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
        let (target, adapter_warnings) = self.subscription_target(document)?;
        if profile_mode(profile) == "remote" {
            let remote = self.remote.render(profile, &target).await?;
            validate_runtime_profile(&remote.profile)?;
            let warnings = adapter_warnings
                .into_iter()
                .chain(remote.warnings)
                .collect();
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
        let compiled = compile(&CompileRequest {
            protocol: 1,
            profile: updated.clone(),
            snapshots,
            custom_nodes: catalog.custom_nodes.clone(),
            target: target.clone(),
        })?;
        Ok(RenderedProfile {
            render: local_render(compiled, adapter_warnings),
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
        let context = self.configuration_context()?;
        let configuration = context.target.ok_or(ManagerError::NoSelectedCore)?;
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

fn local_render(result: CompileResult, mut warnings: Vec<String>) -> SubscriptionRender {
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

fn validate_source_content(content: &str) -> Result<(), SubscriptionError> {
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

fn find_profile<'a>(catalog: &'a Catalog, id: &str) -> Result<&'a Profile, ManagerError> {
    catalog
        .profiles
        .iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| ManagerError::ProfileNotFound(id.into()))
}

fn profile_mode(profile: &Profile) -> &str {
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
    let name = profile
        .transparent_proxy
        .tun
        .get("interface_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if name.is_empty() || name.len() > 15 {
        return Err(SubscriptionError::Invalid(
            "TUN interface name must contain 1 to 15 characters".into(),
        )
        .into());
    }
    Ok(())
}

fn config_build(profile: &Profile, target: &Target) -> Result<ConfigBuild, ManagerError> {
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

fn record_compilation(
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
