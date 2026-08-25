use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use crate::{
    CompileError, CompileRequest, Proxy, SourceSnapshot, icons, normalize_prefix,
    normalized_target, parse_subscription, prepare_profile,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewNode {
    pub name: String,
    #[serde(rename = "type")]
    pub proxy_type: String,
    pub server: String,
    pub port: u16,
    pub source_index: usize,
    pub source_url: String,
    pub raw: Value,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub filtered: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filtered_by: Option<String>,
    #[serde(skip)]
    pub original_name: String,
}

pub fn preview_nodes(request: &CompileRequest) -> Result<Vec<PreviewNode>, CompileError> {
    let target = normalized_target(&request.target)?;
    let profile = prepare_profile(&request.profile, &target)?;
    let mut nodes = Vec::new();
    for value in &profile.manual_servers {
        let proxy = Proxy::from_value(value.clone())
            .map_err(|_| CompileError::InvalidCustomNode("manual server".into()))?;
        nodes.push(preview(proxy, 0, "manual", &[]));
    }
    let selected: std::collections::HashSet<&str> =
        profile.custom_node_ids.iter().map(String::as_str).collect();
    for node in request
        .custom_nodes
        .iter()
        .filter(|node| selected.contains(node.id.as_str()))
    {
        let proxy = Proxy::from_value(node.proxy.clone())
            .map_err(|_| CompileError::InvalidCustomNode(node.name.clone()))?;
        nodes.push(preview(proxy, 0, &format!("custom-node:{}", node.id), &[]));
    }
    let snapshots: HashMap<&str, &SourceSnapshot> = request
        .snapshots
        .iter()
        .map(|snapshot| (snapshot.source_id.as_str(), snapshot))
        .collect();
    for (index, source) in profile.sources.iter().enumerate() {
        if !source.enabled {
            continue;
        }
        let snapshot = snapshots
            .get(source.id.as_str())
            .ok_or_else(|| CompileError::MissingSnapshot(source.id.clone()))?;
        let parsed = parse_subscription(&snapshot.content);
        if parsed.nodes.is_empty() {
            return Err(CompileError::EmptySource {
                label: source.label(),
                detail: parsed.messages().join("; "),
            });
        }
        for mut proxy in parsed.nodes {
            if !source.prefix.trim().is_empty() {
                proxy.name = format!("{}{}", normalize_prefix(&source.prefix), proxy.name);
            }
            nodes.push(preview(proxy, index + 1, &source.url, &profile.filters));
        }
    }
    Ok(nodes)
}

fn preview(
    mut proxy: Proxy,
    source_index: usize,
    source_url: &str,
    filters: &[String],
) -> PreviewNode {
    let original_name = proxy.name.clone();
    proxy.name = icons::append_icon(&proxy.name);
    let filtered_by = filters
        .iter()
        .find(|filter| !filter.is_empty() && proxy.name.contains(filter.as_str()))
        .cloned();
    PreviewNode {
        name: proxy.name.clone(),
        proxy_type: proxy.proxy_type.clone(),
        server: proxy.server.clone(),
        port: proxy.port,
        source_index,
        source_url: source_url.into(),
        raw: proxy.as_value(),
        filtered: filtered_by.is_some(),
        filtered_by,
        original_name,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{Profile, SourceSnapshot, Target};

    #[test]
    fn preview_keeps_filtered_nodes_and_source_positions() {
        let mut profile: Profile = serde_json::from_value(json!({
            "sources": [
                { "id": "disabled", "enabled": false },
                { "id": "source", "enabled": true, "url": "https://example.com", "prefix": "HK" }
            ],
            "filters": ["blocked"]
        }))
        .expect("profile");
        profile
            .extra
            .insert("use_system_filters".into(), json!(false));
        let nodes = preview_nodes(&CompileRequest {
            protocol: 1,
            profile,
            snapshots: vec![SourceSnapshot {
                source_id: "source".into(),
                content:
                    "proxies:\n  - { name: blocked, type: socks5, server: 127.0.0.1, port: 1080 }"
                        .into(),
                content_hash: String::new(),
            }],
            custom_nodes: vec![],
            target: Target::parse("clash-meta").expect("target"),
        })
        .expect("preview");
        assert_eq!(nodes[0].source_index, 2);
        assert!(nodes[0].filtered);
        assert!(nodes[0].name.contains("HK blocked"));
    }
}
