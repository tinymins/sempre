use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    CompileError, CompileRequest, Profile, Proxy, SourceSnapshot, icons, normalize_prefix,
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
        nodes.push(preview_proxy(proxy, 0, "manual", &[]));
    }
    let custom_nodes = request
        .custom_nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    for node in profile
        .custom_node_ids
        .iter()
        .filter_map(|id| custom_nodes.get(id.as_str()).copied())
    {
        let proxy = Proxy::from_value(node.proxy.clone())
            .map_err(|_| CompileError::InvalidCustomNode(node.name.clone()))?;
        nodes.push(preview_proxy(
            proxy,
            0,
            &format!("custom-node:{}", node.id),
            &[],
        ));
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
            nodes.push(preview_proxy(
                proxy,
                index + 1,
                &source.url,
                &profile.filters,
            ));
        }
    }
    Ok(nodes)
}

pub fn trace_node_steps(request: &CompileRequest, name: &str) -> Result<Value, CompileError> {
    let nodes = preview_nodes(request)?;
    let selected = nodes
        .iter()
        .find(|node| node.name == name)
        .ok_or_else(|| CompileError::Inspection(format!("node {name:?} was not found")))?;
    let profile = prepare_profile(&request.profile, &normalized_target(&request.target)?)?;
    let mut original = selected.raw.clone();
    original["name"] = json!(selected.original_name);
    let mut steps = vec![
        json!({ "type": "source", "data": {
            "sourceIndex": selected.source_index, "sourceUrl": selected.source_url,
            "format": source_format(selected), "rawData": original
        }}),
        json!({ "type": "parse", "data": { "clashProxy": original } }),
        json!({ "type": "filter", "data": {
            "passed": !selected.filtered, "matchedRule": selected.filtered_by,
            "filtersApplied": profile.filters
        }}),
        json!({ "type": "enrich", "data": {
            "originalName": selected.original_name, "enrichedName": selected.name
        }}),
    ];
    if !selected.filtered {
        let position = nodes
            .iter()
            .filter(|node| !node.filtered)
            .position(|node| node.name == selected.name)
            .map_or(0, |index| index + 1);
        let active = nodes.iter().filter(|node| !node.filtered).count();
        steps.push(json!({ "type": "merge", "data": {
            "positionInFinalList": position, "totalNodes": active
        }}));
        let groups: Vec<Value> = profile
            .groups
            .iter()
            .filter(|group| !group.readonly || group.proxies.contains(&selected.name))
            .map(|group| json!({ "name": group.name, "type": group.group_type }))
            .collect();
        steps.push(json!({ "type": "group-assign", "data": { "assignedGroups": groups } }));
        if request.target.format.starts_with("sing-box") {
            let mut mini = Profile::default();
            mini.manual_servers.push(selected.raw.clone());
            let converted = crate::compile(&CompileRequest {
                protocol: 1,
                profile: mini,
                snapshots: Vec::new(),
                custom_nodes: Vec::new(),
                target: request.target.clone(),
            })?;
            if let Some(diff) = converted.field_diffs.first() {
                steps.push(json!({ "type": "convert", "data": {
                    "singboxOutbound": diff.outbound,
                    "lostFields": diff.dropped, "ignoredFields": diff.ignored
                }}));
            }
        }
        steps.push(json!({ "type": "output", "data": {
            "configFragment": serde_json::to_string_pretty(&selected.raw)
                .map_err(|error| CompileError::Inspection(error.to_string()))?
        }}));
    }
    Ok(json!({ "nodeName": selected.name, "steps": steps }))
}

fn source_format(node: &PreviewNode) -> &'static str {
    if node.source_index == 0
        || node.source_url == "manual"
        || node.source_url.starts_with("custom-node:")
    {
        "manual"
    } else {
        "yaml"
    }
}

pub fn preview_proxy(
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
