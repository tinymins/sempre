use std::collections::BTreeMap;

use sempre_converter::{
    CompileRequest, FieldDiff, ParseResult, PreviewNode, Profile, Source, Target, compile,
    parse_subscription, preview_nodes, trace_node_steps,
};
use serde::Serialize;
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
    #[serde(skip)]
    pub raw_text: String,
}

#[derive(Clone, Debug)]
pub struct ProfileDebugSource {
    pub source_index: usize,
    pub source: Source,
    pub parse: ParseResult,
    pub raw_text: String,
    pub from_cache: bool,
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
    ) -> Result<ProfileDebugResult, ManagerError> {
        let _operation = self.store.acquire_operation()?;
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
        let mut snapshots = Vec::new();
        let mut sources = Vec::new();
        for (index, source) in updated.sources.iter_mut().enumerate() {
            if !source.enabled {
                continue;
            }
            let result = self
                .fetcher
                .load(source.clone(), true, validate_source_content)
                .await?;
            *source = result.source.clone();
            sources.push(ProfileDebugSource {
                source_index: index + 1,
                source: result.source,
                parse: parse_subscription(&result.snapshot.content),
                raw_text: result.snapshot.content.clone(),
                from_cache: result.from_cache,
            });
            snapshots.push(result.snapshot);
        }
        let request = CompileRequest {
            protocol: 1,
            profile: updated.clone(),
            snapshots,
            custom_nodes: catalog.custom_nodes,
            target: target.clone(),
        };
        let nodes = preview_nodes(&request)?;
        let render = local_render(compile(&request)?, Vec::new());
        let effective = sempre_converter::prepare_profile(&updated, &target)?;
        Ok(ProfileDebugResult {
            profile,
            effective,
            sources,
            nodes,
            render,
        })
    }

    pub async fn render_subscription_profile(
        &self,
        id: &str,
        format: &str,
        force: bool,
    ) -> Result<SubscriptionRender, ManagerError> {
        let _operation = self.store.acquire_operation()?;
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
        let request = self
            .load_profile_request(profile, catalog.custom_nodes, target, force)
            .await?;
        Ok(local_render(compile(&request)?, Vec::new()))
    }

    pub async fn preview_subscription_nodes(
        &self,
        id: &str,
        format: &str,
    ) -> Result<Vec<PreviewNode>, ManagerError> {
        let _operation = self.store.acquire_operation()?;
        let catalog = self.subscriptions.read()?;
        let profile = find_profile(&catalog, id)?.clone();
        if profile_mode(&profile) == "remote" {
            return Err(sempre_subscription::SubscriptionError::Invalid(
                "remote profiles expose compiled artifacts, not editable source nodes".into(),
            )
            .into());
        }
        let request = self
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
        let render = self.render_subscription_profile(id, format, true).await?;
        render
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
        let _operation = self.store.acquire_operation()?;
        let catalog = self.subscriptions.read()?;
        let profile = find_profile(&catalog, id)?.clone();
        if profile_mode(&profile) == "remote" {
            return Err(sempre_subscription::SubscriptionError::Invalid(
                "remote profiles do not expose editable node traces".into(),
            )
            .into());
        }
        let request = self
            .load_profile_request(profile, catalog.custom_nodes, Target::parse(format)?, true)
            .await?;
        Ok(trace_node_steps(&request, name)?)
    }

    pub async fn test_subscription_source(
        &self,
        mut source: Source,
        force: bool,
    ) -> Result<SourceTestResult, ManagerError> {
        let _operation = self.store.acquire_operation()?;
        if source.id.is_empty() {
            source.id = Uuid::new_v4().to_string();
        }
        let result = self
            .fetcher
            .load(source, force, validate_source_content)
            .await?;
        Ok(SourceTestResult {
            parse: parse_subscription(&result.snapshot.content),
            raw_text: result.snapshot.content.clone(),
            source: result.source,
            from_cache: result.from_cache,
            content_hash: result.snapshot.content_hash,
            bytes: result.bytes,
        })
    }

    async fn load_profile_request(
        &self,
        mut profile: Profile,
        custom_nodes: Vec<sempre_converter::CustomNode>,
        target: Target,
        force: bool,
    ) -> Result<CompileRequest, ManagerError> {
        let mut snapshots = Vec::new();
        for source in profile.sources.iter_mut().filter(|source| source.enabled) {
            let result = self
                .fetcher
                .load(source.clone(), force, validate_source_content)
                .await?;
            *source = result.source;
            snapshots.push(result.snapshot);
        }
        Ok(CompileRequest {
            protocol: 1,
            profile,
            snapshots,
            custom_nodes,
            target,
        })
    }
}

#[cfg(test)]
mod tests {
    use sempre_state::{Layout, Store};
    use serde_json::json;

    use super::*;

    fn fixture() -> (tempfile::TempDir, Manager, String) {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = Manager::new(Store::new(Layout::at(root.path()))).expect("manager");
        let mut profile_id = String::new();
        manager
            .subscriptions
            .update(|catalog| {
                let profile = &mut catalog.profiles[0];
                profile_id.clone_from(&profile.id);
                profile.sources.push(
                    serde_json::from_value(json!({
                        "id": "raw", "type": "raw", "enabled": true,
                        "content": "proxies:\n  - { name: edge, type: socks5, server: 127.0.0.1, port: 1080 }"
                    }))
                    .expect("raw source"),
                );
                Ok(())
            })
            .expect("seed profile");
        (root, manager, profile_id)
    }

    #[tokio::test]
    async fn tools_render_preview_trace_and_test_without_runtime_state_changes() {
        let (_root, manager, profile_id) = fixture();
        let before = manager.state().expect("state before");
        let render = manager
            .render_subscription_profile(&profile_id, "clash-meta", false)
            .await
            .expect("render");
        assert_eq!(render.node_count, 1);
        let nodes = manager
            .preview_subscription_nodes(&profile_id, "clash-meta")
            .await
            .expect("preview");
        assert_eq!(nodes[0].name, "edge");
        let trace = manager
            .trace_subscription_node(&profile_id, "edge", "sing-box-v13")
            .await
            .expect("trace");
        assert!(trace.represented);
        let steps = manager
            .trace_subscription_node_steps(&profile_id, "edge", "sing-box-v13")
            .await
            .expect("trace steps");
        assert_eq!(steps["steps"][6]["type"], "convert");
        let tested = manager
            .test_subscription_source(
                serde_json::from_value(json!({
                    "id": "", "type": "raw", "enabled": true,
                    "content": "vless://id@edge.example.com:443?security=tls#edge"
                }))
                .expect("source"),
                true,
            )
            .await
            .expect("source test");
        assert_eq!(tested.parse.nodes.len(), 1);
        assert!(!tested.source.id.is_empty());
        let debug = manager
            .debug_subscription_profile(&profile_id, "clash-meta")
            .await
            .expect("profile debug");
        assert_eq!(debug.sources.len(), 1);
        assert_eq!(debug.sources[0].parse.nodes.len(), 1);
        assert_eq!(debug.nodes.len(), 1);
        assert_eq!(debug.render.node_count, 1);
        assert_eq!(manager.state().expect("state after"), before);
    }
}
