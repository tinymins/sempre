use std::{collections::BTreeMap, sync::Arc, time::Instant};

use axum::{
    Json, Router,
    extract::{Path, State},
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::post,
};
use sempre_converter::{FieldDiff, RuleProvider};
use sempre_manager::ProfileDebugResult;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio::sync::mpsc;

use crate::api::AppState;

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/subscriptions/{id}/debug", post(profile_debug))
}

#[derive(Deserialize)]
struct ProfileDebugInput {
    #[serde(default = "default_format")]
    format: String,
}

struct DebugEvent {
    event: &'static str,
    payload: Value,
}

async fn profile_debug(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<ProfileDebugInput>,
) -> Response {
    let format = if input.format.trim().is_empty() {
        default_format()
    } else {
        input.format
    };
    let (sender, receiver) = mpsc::channel(32);
    tokio::spawn(run_profile_debug(state, id, format, sender));
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
        let item = receiver.recv().await?;
        let event = Event::default().event(item.event).json_data(item.payload);
        Some((event, receiver))
    });
    Sse::new(stream).into_response()
}

async fn run_profile_debug(
    state: Arc<AppState>,
    id: String,
    format: String,
    sender: mpsc::Sender<DebugEvent>,
) {
    let started = Instant::now();
    let result = match state.manager.debug_subscription_profile(&id, &format).await {
        Ok(result) => result,
        Err(error) => {
            send_error(&sender, error.to_string()).await;
            return;
        }
    };
    for event in debug_events(&result, started.elapsed().as_millis()) {
        if sender.send(event).await.is_err() {
            return;
        }
    }
}

fn debug_events(result: &ProfileDebugResult, total_duration_ms: u128) -> Vec<DebugEvent> {
    let mut events = vec![step("config", profile_config(result))];
    let manual_nodes: Vec<_> = result
        .nodes
        .iter()
        .filter(|node| node.source_index == 0)
        .collect();
    events.push(step(
        "manual-servers",
        json!({ "count": manual_nodes.len(), "nodes": manual_nodes }),
    ));
    for source in result
        .profile
        .sources
        .iter()
        .filter(|source| source.enabled)
    {
        let source_index = result
            .profile
            .sources
            .iter()
            .position(|candidate| candidate.id == source.id)
            .map_or(0, |index| index + 1);
        events.push(step(
            "source-start",
            json!({ "sourceIndex": source_index, "url": source.url }),
        ));
    }
    for source in &result.sources {
        let before: Vec<_> = result
            .nodes
            .iter()
            .filter(|node| node.source_index == source.source_index)
            .collect();
        let after: Vec<_> = before
            .iter()
            .copied()
            .filter(|node| !node.filtered)
            .collect();
        let filtered: Vec<_> = before
            .iter()
            .copied()
            .filter(|node| node.filtered)
            .map(|node| {
                json!({ "node": node, "matchedRule": node.filtered_by.as_deref().unwrap_or("") })
            })
            .collect();
        events.push(step(
            "source-result",
            json!({
                "sourceIndex": source.source_index, "url": source.source.url,
                "httpStatus": 200, "httpHeaders": {}, "rawText": source.raw_text,
                "decodedText": nonempty(&source.parse.decoded_text),
                "format": debug_format(&source.parse.format),
                "parsedNodeCount": source.parse.nodes.len(), "nodesBeforeFilter": before,
                "nodesAfterFilter": after, "filteredNodes": filtered, "error": null,
                "fetchDurationMs": 0, "cached": source.from_cache
            }),
        ));
    }
    events.extend(output_events(result, total_duration_ms));
    events
}

fn profile_config(result: &ProfileDebugResult) -> Value {
    let mut providers: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    for provider in &result.effective.rule_providers {
        providers
            .entry(&provider.outbound)
            .or_default()
            .push(json!({ "name": provider.tag, "url": provider.url, "type": provider.behavior }));
    }
    let urls: Vec<_> = result
        .profile
        .sources
        .iter()
        .filter(|source| source.enabled && source.kind == "url")
        .map(|source| &source.url)
        .collect();
    let groups: Vec<_> = result
        .effective
        .groups
        .iter()
        .map(|group| {
            json!({ "name": group.name, "type": group.group_type,
                "proxies": group.proxies, "readonly": group.readonly })
        })
        .collect();
    json!({
        "subscribeUrls": urls, "filters": result.effective.filters,
        "groups": groups, "ruleProviders": providers,
        "customConfig": result.effective.rules, "servers": result.effective.manual_servers,
        "privateAccessConfig": nonempty_object(&result.effective.private_access),
        "dnsConfig": {
            "shared": nested_object(&result.effective.dns, "shared"),
            "overrides": nested_object(&result.effective.dns, "overrides")
        }
    })
}

fn output_events(result: &ProfileDebugResult, total_duration_ms: u128) -> Vec<DebugEvent> {
    let diffs = &result.render.field_diffs;
    let total_filtered = result.nodes.iter().filter(|node| node.filtered).count();
    let final_names: Vec<_> = diffs
        .iter()
        .filter(|diff| diff.outbound.is_some() || diff.dropped.is_empty())
        .map(|diff| &diff.node)
        .collect();
    let warning_nodes = diff_names(diffs, |diff| {
        !diff.dropped.is_empty() || !diff.warnings.is_empty()
    });
    let ignored_nodes = diff_names(diffs, |diff| !diff.ignored.is_empty());
    vec![
        step(
            "merge",
            json!({
                "totalNodesBeforeFilter": diffs.len() + total_filtered,
                "totalNodesAfterFilter": diffs.len(), "totalFiltered": total_filtered,
                "finalNodeNames": final_names, "nodeWarnings": warning_nodes,
                "nodeIgnored": ignored_nodes
            }),
        ),
        step(
            "output",
            json!({
                "proxyGroupCount": result.effective.groups.len(),
                "ruleCount": result.effective.rules.len(),
                "ruleProviderCount": result.effective.rule_providers.len(),
                "configOutput": result.render.content
            }),
        ),
        step("rule-sets", rule_sets(&result.effective.rule_providers)),
        step(
            "validate",
            json!({
                "valid": result.render.runtime_validated, "warnings": result.render.warnings,
                "errors": [], "skipped": !result.render.runtime_validated,
                "reason": "preview does not stage the active runtime", "method": "Sempre compiler"
            }),
        ),
        step("done", json!({ "totalDurationMs": total_duration_ms })),
    ]
}

fn rule_sets(providers: &[RuleProvider]) -> Value {
    let items: Vec<_> = providers
        .iter()
        .map(|provider| {
            json!({
                "tag": provider.tag, "url": provider.url, "effectiveUrl": provider.url,
                "group": provider.outbound, "status": "ok", "ruleCount": 0,
                "sampleRules": [], "builtin": false, "format": provider.format
            })
        })
        .collect();
    json!({ "totalCount": items.len(), "totalRules": 0, "errorCount": 0, "items": items })
}

fn diff_names<F>(diffs: &[FieldDiff], predicate: F) -> Vec<&str>
where
    F: Fn(&FieldDiff) -> bool,
{
    diffs
        .iter()
        .filter(|diff| predicate(diff))
        .map(|diff| diff.node.as_str())
        .collect()
}

fn nested_object(value: &Value, key: &str) -> Value {
    value
        .get(key)
        .and_then(Value::as_object)
        .cloned()
        .map_or_else(|| json!({}), Value::Object)
}

fn nonempty_object(value: &Value) -> Option<&Map<String, Value>> {
    value.as_object().filter(|object| !object.is_empty())
}

fn step(kind: &'static str, data: Value) -> DebugEvent {
    let payload = Map::from_iter([
        ("type".into(), Value::String(kind.into())),
        ("data".into(), data),
    ]);
    DebugEvent {
        event: "message",
        payload: Value::Object(payload),
    }
}

async fn send_error(sender: &mpsc::Sender<DebugEvent>, message: String) {
    let _ = sender
        .send(DebugEvent {
            event: "error",
            payload: json!({ "message": message }),
        })
        .await;
}

fn debug_format(value: &str) -> &str {
    match value {
        "base64" => "base64",
        "yaml" => "yaml",
        _ => "unknown",
    }
}

fn nonempty(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

fn default_format() -> String {
    "sing-box-v13".into()
}

#[cfg(test)]
mod tests {
    use sempre_converter::CompileRequest;

    use super::*;

    #[test]
    fn debug_steps_match_the_ui_contract() {
        let fixture = serde_json::from_value::<CompileRequest>(json!({
            "profile": {
                "id": "profile", "sources": [{
                    "id": "source", "type": "raw", "enabled": true, "content": ""
                }]
            },
            "snapshots": [{
                "source_id": "source",
                "content": "proxies:\n  - { name: edge, type: socks5, server: 127.0.0.1, port: 1080 }"
            }],
            "custom_nodes": [], "target": { "format": "clash-meta" }
        }))
        .expect("request");
        let effective = sempre_converter::prepare_profile(&fixture.profile, &fixture.target)
            .expect("effective profile");
        let parse = sempre_converter::parse_subscription(&fixture.snapshots[0].content);
        let nodes = sempre_converter::preview_nodes(&fixture).expect("nodes");
        let compiled = sempre_converter::compile(&fixture).expect("compile");
        let result = ProfileDebugResult {
            profile: fixture.profile,
            effective,
            sources: vec![sempre_manager::ProfileDebugSource {
                source_index: 1,
                source: serde_json::from_value(json!({
                    "id": "source", "type": "raw", "enabled": true
                }))
                .expect("source"),
                parse,
                raw_text: fixture.snapshots[0].content.clone(),
                from_cache: false,
            }],
            nodes,
            render: sempre_manager::SubscriptionRender {
                format: compiled.format,
                version: compiled.version,
                platform: compiled.platform,
                content: compiled.content,
                artifact_hash: compiled.artifact_hash,
                node_count: compiled.node_count,
                field_diffs: compiled.field_diffs,
                node_origins: compiled.node_origins.into_iter().collect(),
                diagnostics: compiled.diagnostics,
                warnings: Vec::new(),
                runtime_validated: compiled.runtime_validated,
            },
        };
        let events = debug_events(&result, 7);
        let types: Vec<_> = events
            .iter()
            .filter_map(|event| event.payload["type"].as_str())
            .collect();
        assert_eq!(
            types,
            [
                "config",
                "manual-servers",
                "source-start",
                "source-result",
                "merge",
                "output",
                "rule-sets",
                "validate",
                "done"
            ]
        );
        assert_eq!(events[3].payload["data"]["parsedNodeCount"], 1);
        assert_eq!(events[8].payload["data"]["totalDurationMs"], 7);
    }
}
