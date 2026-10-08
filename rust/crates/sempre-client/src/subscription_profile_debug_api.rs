use std::{collections::BTreeMap, sync::Arc, time::Instant};

use axum::{
    Json, Router,
    extract::{Path, State},
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::post,
};
use sempre_converter::{FieldDiff, Profile, RuleProvider};
use sempre_manager::{ProfileDebugProgress, ProfileDebugResult};
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
    let (progress_sender, mut progress_receiver) = mpsc::channel(32);
    let work = state
        .manager
        .debug_subscription_profile(&id, &format, progress_sender);
    tokio::pin!(work);
    let result = loop {
        tokio::select! {
            () = sender.closed() => return,
            progress = progress_receiver.recv() => {
                if let Some(progress) = progress
                    && sender.send(progress_event(progress)).await.is_err()
                {
                    return;
                }
            }
            result = &mut work => break result,
        }
    };
    while let Ok(progress) = progress_receiver.try_recv() {
        if sender.send(progress_event(progress)).await.is_err() {
            return;
        }
    }
    let result = match result {
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
    let mut events = Vec::new();
    let manual_nodes: Vec<_> = result
        .nodes
        .iter()
        .filter(|node| node.source_index == 0)
        .collect();
    events.push(step(
        "manual-servers",
        json!({ "count": manual_nodes.len(), "nodes": manual_nodes }),
    ));
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
                "httpStatus": source.observation.http_status,
                "httpHeaders": source.observation.http_headers,
                "rawText": source.raw_text,
                "decodedText": nonempty(&source.parse.decoded_text),
                "format": debug_format(&source.parse.format),
                "parsedNodeCount": source.parse.nodes.len(), "nodesBeforeFilter": before,
                "nodesAfterFilter": after, "filteredNodes": filtered, "error": null,
                "fetchDurationMs": source.observation.fetch_duration_ms,
                "cached": source.from_cache
            }),
        ));
    }
    events.extend(output_events(result, total_duration_ms));
    events
}

fn progress_event(progress: ProfileDebugProgress) -> DebugEvent {
    match progress {
        ProfileDebugProgress::Configured { profile, effective } => {
            step("config", profile_config(&profile, &effective))
        }
        ProfileDebugProgress::SourceStarted {
            source_index,
            source,
        } => step(
            "source-start",
            json!({ "sourceIndex": source_index, "url": source.url }),
        ),
        ProfileDebugProgress::SourceFetched(source) => step(
            "source-fetched",
            json!({
                "sourceIndex": source.source_index, "url": source.source.url,
                "httpStatus": source.observation.http_status,
                "httpHeaders": source.observation.http_headers,
                "fetchDurationMs": source.observation.fetch_duration_ms,
                "parsedNodeCount": source.parse.nodes.len(),
                "cached": source.from_cache, "error": null
            }),
        ),
        ProfileDebugProgress::SourceFailed {
            source_index,
            source,
            error,
            observation,
        } => step(
            "source-failed",
            json!({
                "sourceIndex": source_index, "url": source.url,
                "httpStatus": observation.http_status,
                "httpHeaders": observation.http_headers,
                "fetchDurationMs": observation.fetch_duration_ms,
                "cached": false, "error": error
            }),
        ),
        ProfileDebugProgress::RulesStarted => step("rule-sets-start", json!({})),
        ProfileDebugProgress::RulesFinished {
            providers,
            snapshot_ids,
            warnings,
        } => step("rule-sets", rule_sets(&providers, &snapshot_ids, &warnings)),
        ProfileDebugProgress::Compiling => step("compile-start", json!({})),
    }
}

fn profile_config(profile: &Profile, effective: &Profile) -> Value {
    let mut providers: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    for provider in &effective.rule_providers {
        providers
            .entry(&provider.outbound)
            .or_default()
            .push(json!({ "name": provider.tag, "url": provider.url, "type": provider.behavior }));
    }
    let urls: Vec<_> = profile
        .sources
        .iter()
        .filter(|source| source.enabled && source.kind == "url")
        .map(|source| &source.url)
        .collect();
    let groups: Vec<_> = effective
        .groups
        .iter()
        .map(|group| {
            json!({ "name": group.name, "type": group.group_type,
                "proxies": group.proxies, "readonly": group.readonly })
        })
        .collect();
    json!({
        "subscribeUrls": urls, "filters": effective.filters,
        "groups": groups, "ruleProviders": providers,
        "customConfig": effective.rules, "servers": effective.manual_servers,
        "privateAccessConfig": nonempty_object(&effective.private_access),
        "dnsConfig": {
            "shared": nested_object(&effective.dns, "shared")
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

fn rule_sets(providers: &[RuleProvider], snapshot_ids: &[String], warnings: &[String]) -> Value {
    let items: Vec<_> = providers
        .iter()
        .map(|provider| {
            let snapshot_id = sempre_converter::rule_provider_snapshot_id(&provider.tag);
            let available = snapshot_ids.contains(&snapshot_id);
            let error = warnings
                .iter()
                .find(|message| message.contains(&format!("{:?}", provider.tag)));
            json!({
                "tag": provider.tag, "url": provider.url, "effectiveUrl": provider.url,
                "group": provider.outbound,
                "status": if available { "snapshot" } else if error.is_some() { "error" } else { "remote" },
                "ruleCount": null, "sampleRules": [], "builtin": false,
                "format": provider.format, "error": error
            })
        })
        .collect();
    json!({ "totalCount": items.len(), "totalRules": null,
        "errorCount": warnings.len(), "items": items })
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
