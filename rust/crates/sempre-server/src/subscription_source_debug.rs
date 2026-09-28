use std::sync::Arc;

use axum::{Json, Router, extract::State, response::Response, routing::post};
use sempre_converter::parse_subscription;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    auth::CurrentUser,
    debug_stream::{self, StageLog},
    error::ApiError,
    source_cache::{self, CacheMode, LoadedSource, SourceKind, SourceRequest},
    subscription_compile::saved_input,
    subscription_sources::{source_id, source_items, source_proxy},
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
    subscription_id: Option<Uuid>,
    source_index: Option<usize>,
}

fn validate_input(input: &DebugSourceInput) -> Result<(), ApiError> {
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
    if input.subscription_id.is_some() != input.source_index.is_some() {
        return Err(ApiError::bad_request(
            "subscriptionId and sourceIndex must be provided together",
        ));
    }
    if input.mode.as_deref() == Some("production") && input.subscription_id.is_none() {
        return Err(ApiError::bad_request(
            "production source debug requires subscriptionId and sourceIndex",
        ));
    }
    Ok(())
}

async fn debug_source(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<DebugSourceInput>,
) -> Result<Response, ApiError> {
    validate_input(&input)?;
    let saved = if input.mode.as_deref() == Some("production") {
        if let (Some(id), Some(index)) = (input.subscription_id, input.source_index) {
            let (fields, _) = saved_input(&state, id, user.id).await?;
            let item = source_items(&fields)?
                .into_iter()
                .nth(index)
                .ok_or_else(|| ApiError::bad_request("saved source index is invalid"))?;
            if !item.enabled {
                return Err(ApiError::bad_request("saved source is disabled"));
            }
            Some((id, fields.cache_ttl_minutes, item))
        } else {
            None
        }
    } else {
        None
    };
    let url = saved
        .as_ref()
        .map_or(input.url.as_str(), |(_, _, item)| item.url.as_str());
    let ua = if let Some((_, _, item)) = &saved {
        Some(item.effective_ua())
    } else {
        input.ua.as_deref().filter(|value| !value.trim().is_empty())
    }
    .unwrap_or("clash.meta")
    .to_owned();
    let fetch_mode = if let Some((_, _, item)) = &saved {
        item.fetch_mode.as_deref()
    } else {
        input.fetch_mode.as_deref()
    }
    .unwrap_or("auto");
    let ttl = saved.as_ref().map_or(
        input.cache_ttl_minutes.unwrap_or(60),
        |(_, default_ttl, item)| item.cache_ttl_minutes.or(*default_ttl).unwrap_or(60),
    );
    let proxy = source_proxy(&state, fetch_mode)?.map(str::to_owned);
    let identity = source_id(url, &ua, fetch_mode);
    let prefix = saved.as_ref().map_or_else(
        || input.prefix.clone().unwrap_or_default(),
        |(_, _, item)| item.prefix.clone(),
    );
    let cache_mode = if input.mode.as_deref() == Some("production") {
        CacheMode::ReadOnlySubscription(saved.as_ref().expect("production source checked").0)
    } else {
        CacheMode::Bypass
    };
    let url = url.to_owned();
    let fetch_mode = fetch_mode.to_owned();
    Ok(debug_stream::response(move |mut stages| async move {
        stages.push(json!({"type":"source","status":"running","cacheMode":if matches!(cache_mode, CacheMode::Bypass) { "bypass" } else { "production" }}));
        stages.push(json!({"type":"request","status":"ok","ua":ua,"fetchMode":fetch_mode,"cacheTtlMinutes":ttl,"prefix":prefix}));
        let loaded = source_cache::load_observed(
            &state,
            SourceRequest {
                url: &url,
                ua: &ua,
                fetch_mode: &fetch_mode,
                proxy: proxy.as_deref(),
                source_id: &identity,
                ttl_minutes: ttl,
                mode: cache_mode,
                kind: SourceKind::Nodes,
                inspect_unusable: true,
            },
            &mut stages,
        )
        .await;
        match loaded {
            Ok(loaded) => {
                stages.push(json!({"type":"source","status":if loaded.usable { "ok" } else { "error" },"httpStatus":loaded.http_status,"cacheState":loaded.cache_state}));
                debug_result(&loaded, &ua, &fetch_mode, ttl, &prefix, stages)
            }
            Err(error) => {
                stages.push(json!({"type":"source","status":"error","message":error.message()}));
                error_result(
                    &error,
                    &ua,
                    &fetch_mode,
                    ttl,
                    &prefix,
                    matches!(cache_mode, CacheMode::ReadOnlySubscription(_)),
                    stages,
                )
            }
        }
    }))
}

fn error_result(
    error: &ApiError,
    ua: &str,
    fetch_mode: &str,
    ttl: i32,
    prefix: &str,
    production: bool,
    mut stages: StageLog,
) -> Value {
    stages.push(json!({"type":"complete","status":"error","resultSource":null,"nodeCount":0,"message":error.message()}));
    json!({
        "ok": false,
        "message": error.message(),
        "status": null,
        "responseHeaders": {},
        "raw": "",
        "rawTruncated": false,
        "decodedText": "",
        "decodedTextTruncated": false,
        "decoded": [],
        "ua": ua,
        "fetchMode": fetch_mode,
        "cacheTtlMinutes": ttl,
        "prefix": prefix,
        "nodeCount": 0,
        "nodes": [],
        "bodyBytes": 0,
        "cached": false,
        "cacheState": if production { "miss" } else { "bypass" },
        "diagnostics": [{"level":"error","message":error.message()}],
        "stages": stages.into_events(),
    })
}

fn debug_result(
    loaded: &LoadedSource,
    ua: &str,
    fetch_mode: &str,
    ttl: i32,
    prefix: &str,
    stages: StageLog,
) -> Value {
    let parsed = parse_subscription(&loaded.content);
    let mut stages = stages;
    stages.push(json!({"type":"parse","status":if parsed.nodes.is_empty() { "error" } else { "ok" },"format":parsed.format,"parsedNodeCount":parsed.nodes.len(),"diagnostics":parsed.diagnostics,"bodyBytes":loaded.content.len()}));
    stages.push(json!({"type":"complete","status":if loaded.usable { "ok" } else { "error" },"resultSource":match loaded.cache_state { "fresh" => "cache", "stale" => "stale-cache", _ => "live" },"nodeCount":parsed.nodes.len()}));
    let mut diagnostics = parsed
        .diagnostics
        .iter()
        .map(|message| json!({"level":"warning","message":message}))
        .collect::<Vec<_>>();
    if !loaded.usable {
        diagnostics.insert(0, json!({"level":"error","message":loaded.warning.as_deref().unwrap_or("source has no usable nodes")}));
    }
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
    json!({
        "ok": loaded.usable,
        "message": if loaded.usable { None } else { loaded.warning.as_deref() },
        "status": loaded.http_status,
        "responseHeaders": loaded.response_headers,
        "raw": loaded.content.chars().take(65536).collect::<String>(),
        "rawTruncated": loaded.content.chars().count() > 65536,
        "decodedText": parsed.decoded_text.chars().take(65536).collect::<String>(),
        "decodedTextTruncated": parsed.decoded_text.chars().count() > 65536,
        "decoded": parsed.nodes.iter().map(sempre_converter::Proxy::as_value).collect::<Vec<_>>(),
        "ua": ua,
        "fetchMode": fetch_mode,
        "cacheTtlMinutes": ttl,
        "prefix": prefix,
        "nodeCount": nodes.len(),
        "nodes": nodes,
        "bodyBytes": loaded.content.len(),
        "cached": matches!(loaded.cache_state, "fresh" | "stale"),
        "cacheState": loaded.cache_state,
        "warning": loaded.warning.clone(),
        "diagnostics": diagnostics,
        "stages": stages.into_events(),
    })
}
