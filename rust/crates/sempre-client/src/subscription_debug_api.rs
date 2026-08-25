use std::{collections::HashSet, sync::Arc, time::Duration, time::Instant};

use axum::{
    Json, Router,
    extract::State,
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::post,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{net::TcpStream, sync::mpsc};
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
    let fetch_started = Instant::now();
    let source = serde_json::from_value(json!({
        "id": "", "type": "url", "enabled": true, "url": input.url,
        "prefix": input.prefix, "user_agent": input.ua,
        "fetch_mode": input.fetch_mode, "cache_ttl_minutes": input.cache_ttl_minutes
    }));
    let result = match source {
        Ok(source) => {
            state
                .manager
                .test_subscription_source(source, input.mode != "production")
                .await
        }
        Err(error) => {
            send_error_result(
                &sender,
                &input.url,
                &error.to_string(),
                fetch_started,
                started,
            )
            .await;
            return;
        }
    };
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            send_error_result(
                &sender,
                &input.url,
                &error.to_string(),
                fetch_started,
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
                "attempt": 1, "maxAttempts": 3, "success": true, "httpStatus": 200,
                "finalUrl": input.url, "httpHeaders": {},
                "fetchDurationMs": millis(fetch_started), "error": null, "requestError": null,
                "remoteAddress": null, "httpVersion": "HTTP", "tlsPeerCertificateBytes": null,
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
        "miss"
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
        step(
            "network",
            network_diagnostics(&input.url, &input.fetch_mode).await,
        ),
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
    fetch_started: Instant,
    started: Instant,
) {
    if !send(
        sender,
        step(
            "attempt-result",
            json!({
                "attempt": 1, "maxAttempts": 3, "success": false, "httpStatus": null,
                "finalUrl": url, "httpHeaders": {}, "fetchDurationMs": millis(fetch_started),
                "error": message, "requestError": {
                    "message": message, "debug": message, "chain": [message],
                    "isTimeout": false, "isConnect": true, "isRequest": true,
                    "isBody": false, "isDecode": false, "status": null, "url": url
                },
                "remoteAddress": null, "httpVersion": null, "tlsPeerCertificateBytes": null,
                "payload": empty_payload()
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

async fn network_diagnostics(raw_url: &str, fetch_mode: &str) -> Value {
    let started = Instant::now();
    let parsed = Url::parse(raw_url).ok();
    let scheme = parsed.as_ref().map(Url::scheme);
    let host = parsed.as_ref().and_then(Url::host_str);
    let port = parsed.as_ref().and_then(Url::port_or_known_default);
    let mut addresses = Vec::new();
    let mut dns_error = None;
    if let (Some(host), Some(port)) = (host, port) {
        match tokio::net::lookup_host((host, port)).await {
            Ok(values) => {
                let mut seen = HashSet::new();
                addresses.extend(values.filter(|value| seen.insert(value.ip())).take(3));
            }
            Err(error) => dns_error = Some(error.to_string()),
        }
    }
    let mut probes = Vec::new();
    for address in &addresses {
        let probe_started = Instant::now();
        let connected =
            tokio::time::timeout(Duration::from_secs(2), TcpStream::connect(address)).await;
        match connected {
            Ok(Ok(stream)) => probes.push(json!({
                "address": address.ip(), "success": true, "durationMs": millis(probe_started),
                "localAddress": stream.local_addr().ok().map(|value| value.to_string()),
                "remoteAddress": stream.peer_addr().ok().map(|value| value.to_string()), "error": null
            })),
            Ok(Err(error)) => probes.push(failed_probe(
                &address.ip().to_string(),
                millis(probe_started),
                &error.to_string(),
            )),
            Err(_) => probes.push(failed_probe(
                &address.ip().to_string(),
                millis(probe_started),
                "connection timed out",
            )),
        }
    }
    json!({
        "fetchMode": fetch_mode, "connectionKind": "origin", "proxyEndpoint": null,
        "scheme": scheme, "host": host, "port": port, "resolverConfig": [],
        "proxyEnvironmentVariables": [], "dnsDurationMs": millis(started),
        "resolvedAddresses": addresses.iter().map(|value| value.ip().to_string()).collect::<Vec<_>>(),
        "dnsError": dns_error, "tcpProbes": probes
    })
}

fn failed_probe(address: &str, duration: u128, error: &str) -> Value {
    json!({ "address": address, "success": false, "durationMs": duration,
        "localAddress": null, "remoteAddress": null, "error": error })
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

fn empty_payload() -> Value {
    json!({ "format": "unknown", "rawText": "", "decodedText": null,
        "bodyBytes": 0, "parsedNodeCount": 0, "nodes": [],
        "discardedPlaceholderNodes": [], "diagnostics": [] })
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
