use std::collections::BTreeMap;

use sempre_converter::{
    CompileRequest, FieldDiff, ParseResult, PreviewNode, Profile, Source, Target, compile,
    parse_subscription, preview_nodes,
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
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
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
        assert_eq!(manager.state().expect("state after"), before);
    }
}
