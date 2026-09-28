use std::{sync::Arc, time::Instant};

use axum::{
    Json, Router,
    extract::State,
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::post,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use url::Url;

use crate::api::AppState;

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/subscriptions/source/debug", post(source_debug))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceDebugInput {
    url: String,
    #[serde(default = "default_user_agent")]
    ua: String,
    #[serde(default)]
    prefix: String,
    #[serde(default)]
    cache_ttl_minutes: i64,
    mode: String,
    #[serde(default = "default_fetch_mode")]
    fetch_mode: String,
}

struct DebugEvent {
    event: &'static str,
    payload: Value,
}

async fn source_debug(
    State(state): State<Arc<AppState>>,
    Json(mut input): Json<SourceDebugInput>,
) -> Response {
    if !matches!(input.mode.as_str(), "bypass-cache" | "production") {
        return crate::subscription_api::operation(
            "source debug mode must be bypass-cache or production",
        );
    }
    if input.ua.trim().is_empty() {
        input.ua = default_user_agent();
    }
    if input.fetch_mode.trim().is_empty() {
        input.fetch_mode = default_fetch_mode();
    }
    let (sender, receiver) = mpsc::channel(16);
    tokio::spawn(run_source_debug(state, input, sender));
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
        let item = receiver.recv().await?;
        let event = Event::default().event(item.event).json_data(item.payload);
        Some((event, receiver))
    });
    Sse::new(stream).into_response()
}

async fn run_source_debug(
    state: Arc<AppState>,
    input: SourceDebugInput,
    sender: mpsc::Sender<DebugEvent>,
) {
    let started = Instant::now();
    if !send_source_prelude(&sender, &input).await {
        return;
    }
    let source = serde_json::from_value(json!({
        "id": "", "type": "url", "enabled": true, "url": input.url,
        "prefix": input.prefix, "user_agent": input.ua,
        "fetch_mode": input.fetch_mode, "cache_ttl_minutes": input.cache_ttl_minutes
    }));
    let inspection = match source {
        Ok(source) => {
            tokio::select! {
                () = sender.closed() => return,
                inspection = state.manager.inspect_subscription_source(source, input.mode != "production") => inspection,
            }
        }
        Err(error) => {
            send_error_result(
                &sender,
                &input.url,
                &error.to_string(),
                &sempre_subscription::FetchObservation::default(),
                started,
            )
            .await;
            return;
        }
    };
    let result = match inspection.result {
        Ok(result) => result,
        Err(error) => {
            send_error_result(
                &sender,
                &input.url,
                &error.to_string(),
                &inspection.observation,
                started,
            )
            .await;
            return;
        }
    };
    let payload = source_payload(&result);
    if !send(
        &sender,
        step(
            "attempt-result",
            json!({
                "attempt": result.observation.attempts, "maxAttempts": 3,
                "success": true, "httpStatus": result.observation.http_status,
                "finalUrl": result.observation.final_url,
                "httpHeaders": result.observation.http_headers,
                "fetchDurationMs": result.observation.fetch_duration_ms,
                "error": null, "requestError": null,
                "remoteAddress": null, "httpVersion": null, "tlsPeerCertificateBytes": null,
                "payload": payload
            }),
        ),
    )
    .await
    {
        return;
    }
    let result_source = if !result.from_cache {
        "live"
    } else if result.source.extra.get("last_status") == Some(&json!("last-known-good cache")) {
        "stale-cache"
    } else {
        "cache"
    };
    send(
        &sender,
        step(
            "done",
            json!({
                "success": true, "resultSource": result_source,
                "nodeCount": result.parse.nodes.len(), "totalDurationMs": millis(started)
            }),
        ),
    )
    .await;
}

async fn send_source_prelude(sender: &mpsc::Sender<DebugEvent>, input: &SourceDebugInput) -> bool {
    if !send(
        sender,
        step(
            "config",
            json!({
                "url": input.url, "ua": input.ua, "prefix": input.prefix,
                "cacheTtlMinutes": input.cache_ttl_minutes, "mode": input.mode,
                "fetchMode": input.fetch_mode, "proxyEndpoint": null,
                "maxAttempts": 3, "timeoutMs": 15000
            }),
        ),
    )
    .await
    {
        return false;
    }
    let cache_status = if input.mode == "production" {
        "checking"
    } else {
        "skipped"
    };
    if !send(
        sender,
        step(
            "cache",
            json!({
                "status": cache_status, "cacheTtlMinutes": input.cache_ttl_minutes,
                "payload": null
            }),
        ),
    )
    .await
    {
        return false;
    }
    if !send(
        sender,
        step("network", network_context(&input.url, &input.fetch_mode)),
    )
    .await
        || !send(
            sender,
            step("attempt-start", json!({ "attempt": 1, "maxAttempts": 3 })),
        )
        .await
    {
        return false;
    }
    true
}

async fn send_error_result(
    sender: &mpsc::Sender<DebugEvent>,
    url: &str,
    message: &str,
    observation: &sempre_subscription::FetchObservation,
    started: Instant,
) {
    if !send(
        sender,
        step(
            "attempt-result",
            json!({
                "attempt": observation.attempts, "maxAttempts": 3,
                "success": false, "httpStatus": observation.http_status,
                "finalUrl": observation.final_url, "httpHeaders": observation.http_headers,
                "fetchDurationMs": observation.fetch_duration_ms,
                "error": message, "requestError": {
                    "message": message, "debug": message, "chain": [message],
                    "isTimeout": null, "isConnect": null, "isRequest": null,
                    "isBody": null, "isDecode": null,
                    "status": observation.http_status, "url": url
                },
                "remoteAddress": null, "httpVersion": null, "tlsPeerCertificateBytes": null,
                "payload": error_payload(observation)
            }),
        ),
    )
    .await
    {
        return;
    }
    if !send(
        sender,
        step("fallback", json!({ "status": "miss", "payload": null })),
    )
    .await
    {
        return;
    }
    send(
        sender,
        step(
            "done",
            json!({
                "success": false, "resultSource": null, "nodeCount": 0,
                "totalDurationMs": millis(started)
            }),
        ),
    )
    .await;
}

fn source_payload(result: &sempre_manager::SourceTestResult) -> Value {
    let source_url = &result.source.url;
    let nodes: Vec<_> = result
        .parse
        .nodes
        .iter()
        .cloned()
        .map(|proxy| sempre_converter::preview_proxy(proxy, 1, source_url, &[]))
        .collect();
    let discarded: Vec<_> = result
        .parse
        .discarded_placeholder_nodes
        .iter()
        .cloned()
        .map(|proxy| sempre_converter::preview_proxy(proxy, 1, source_url, &[]))
        .collect();
    json!({
        "format": debug_format(&result.parse.format), "rawText": result.raw_text,
        "decodedText": nonempty(&result.parse.decoded_text), "bodyBytes": result.bytes,
        "parsedNodeCount": nodes.len(), "nodes": nodes,
        "discardedPlaceholderNodes": discarded, "diagnostics": result.parse.diagnostics
    })
}

fn network_context(raw_url: &str, fetch_mode: &str) -> Value {
    let parsed = Url::parse(raw_url).ok();
    let scheme = parsed.as_ref().map(Url::scheme);
    let host = parsed.as_ref().and_then(Url::host_str);
    let port = parsed.as_ref().and_then(Url::port_or_known_default);
    json!({
        "fetchMode": fetch_mode, "connectionKind": null, "proxyEndpoint": null,
        "scheme": scheme, "host": host, "port": port, "resolverConfig": [],
        "proxyEnvironmentVariables": [], "dnsDurationMs": null,
        "resolvedAddresses": [], "dnsError": null, "tcpProbes": [],
        "note": "network path was not separately probed; HTTP evidence comes from the fetch"
    })
}

fn step(kind: &'static str, data: Value) -> DebugEvent {
    let payload = serde_json::Map::from_iter([
        ("type".into(), Value::String(kind.into())),
        ("data".into(), data),
    ]);
    DebugEvent {
        event: "message",
        payload: Value::Object(payload),
    }
}

async fn send(sender: &mpsc::Sender<DebugEvent>, event: DebugEvent) -> bool {
    sender.send(event).await.is_ok()
}

fn error_payload(observation: &sempre_subscription::FetchObservation) -> Value {
    let raw = observation.body_preview.as_deref().unwrap_or_default();
    let parsed = sempre_converter::parse_subscription(raw);
    json!({
        "format": debug_format(&parsed.format),
        "rawText": raw, "rawTruncated": observation.body_truncated,
        "decodedText": nonempty(&parsed.decoded_text),
        "bodyBytes": observation.body_bytes,
        "parsedNodeCount": parsed.nodes.len(), "nodes": [],
        "discardedPlaceholderNodes": [], "diagnostics": parsed.diagnostics
    })
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

fn millis(started: Instant) -> u128 {
    started.elapsed().as_millis()
}

fn default_user_agent() -> String {
    "clash.meta".into()
}

fn default_fetch_mode() -> String {
    "auto".into()
}
