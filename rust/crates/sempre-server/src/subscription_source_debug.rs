use std::{sync::Arc, time::Instant};

use axum::{Json, Router, extract::State, routing::post};
use sempre_converter::parse_subscription;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AppState,
    auth::CurrentUser,
    error::ApiError,
    source_cache::{self, CacheMode, SourceKind, SourceRequest},
    subscription_sources::source_proxy,
};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/subscriptions/debug-source", post(debug_source))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DebugSourceInput {
    url: String,
    ua: Option<String>,
    prefix: Option<String>,
    cache_ttl_minutes: Option<i32>,
    fetch_mode: Option<String>,
    mode: Option<String>,
}

async fn debug_source(
    State(state): State<Arc<AppState>>,
    CurrentUser(_user): CurrentUser,
    Json(input): Json<DebugSourceInput>,
) -> Result<Json<Value>, ApiError> {
    if input
        .mode
        .as_deref()
        .is_some_and(|mode| !matches!(mode, "production" | "bypass-cache"))
    {
        return Err(ApiError::bad_request("invalid mode"));
    }
    if input.cache_ttl_minutes.is_some_and(|ttl| ttl < 0) {
        return Err(ApiError::bad_request("cacheTtlMinutes must be nonnegative"));
    }
    let ua = input
        .ua
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "clash.meta".into());
    let fetch_mode = input.fetch_mode.as_deref().unwrap_or("auto");
    let proxy = source_proxy(&state, fetch_mode)?;
    let started = Instant::now();
    let loaded = source_cache::load(
        &state,
        SourceRequest {
            url: &input.url,
            ua: &ua,
            fetch_mode,
            proxy,
            source_id: "debug-source",
            ttl_minutes: input.cache_ttl_minutes.unwrap_or(60),
            mode: if input.mode.as_deref() == Some("bypass-cache") {
                CacheMode::Bypass
            } else {
                CacheMode::Global
            },
            kind: SourceKind::Nodes,
        },
    )
    .await;
    let loaded = match loaded {
        Ok(loaded) => loaded,
        Err(error) => {
            return Ok(Json(json!({
                "ok": false,
                "status": 0,
                "ua": ua,
                "nodeCount": 0,
                "nodes": [],
                "elapsedMs": started.elapsed().as_millis(),
                "bodyBytes": 0,
                "cached": false,
                "cacheState": if input.mode.as_deref() == Some("bypass-cache") { "bypass" } else { "miss" },
                "diagnostics": [{"level":"error","message":error.message()}],
            })));
        }
    };
    let parsed = parse_subscription(&loaded.content);
    let prefix = input.prefix.unwrap_or_default();
    let nodes = parsed
        .nodes
        .iter()
        .map(|node| {
            let value = node.as_value();
            json!({
                "name": format!("{}{}", prefix, node.name),
                "type": value.get("type"),
                "server": value.get("server"),
                "port": value.get("port"),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "ok": true,
        "status": 200,
        "ua": ua,
        "nodeCount": nodes.len(),
        "nodes": nodes,
        "elapsedMs": started.elapsed().as_millis(),
        "bodyBytes": loaded.content.len(),
        "cached": matches!(loaded.cache_state, "fresh" | "stale"),
        "cacheState": loaded.cache_state,
        "warning": loaded.warning,
        "diagnostics": parsed.diagnostics,
    })))
}
