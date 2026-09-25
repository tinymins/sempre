use sempre_converter::{Diagnostic, Profile, Source, SourceSnapshot};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::{
    AppState,
    error::ApiError,
    source_cache::{self, CacheMode, SourceKind, SourceRequest},
    subscriptions::SubscriptionFields,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceItem {
    #[serde(default = "default_true")]
    enabled: bool,
    url: String,
    #[serde(default)]
    prefix: String,
    #[serde(default)]
    remark: String,
    cache_ttl_minutes: Option<i32>,
    fetch_ua: Option<String>,
    fetch_mode: Option<String>,
}

fn default_true() -> bool {
    true
}

pub(crate) async fn load_sources(
    state: &AppState,
    fields: &SubscriptionFields,
    profile: &mut Profile,
    cache_mode: CacheMode,
    stages: &mut Vec<Value>,
    snapshots: &mut Vec<SourceSnapshot>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), ApiError> {
    let items = source_items(fields)?;
    let enabled_items = items
        .into_iter()
        .filter(|item| item.enabled)
        .collect::<Vec<_>>();
    if enabled_items.is_empty() {
        stages.push(
            json!({"type":"fetch","status":"skipped","message":"no enabled subscription sources"}),
        );
    }
    for item in enabled_items {
        if item.url.trim().is_empty() {
            return Err(ApiError::bad_request("enabled source URL is empty"));
        }
        let ua = item.fetch_ua.unwrap_or_else(|| "clash.meta".into());
        let mode = item.fetch_mode.unwrap_or_else(|| "auto".into());
        let proxy = source_proxy(state, &mode)?;
        let source_id = format!(
            "{:x}",
            Sha256::digest(format!("{}\0{}\0{}", item.url, ua, mode).as_bytes())
        );
        let ttl = item
            .cache_ttl_minutes
            .or(fields.cache_ttl_minutes)
            .unwrap_or(60);
        if ttl < 0 {
            return Err(ApiError::bad_request("cache TTL must be nonnegative"));
        }
        let loaded = source_cache::load(state, SourceRequest {
            url: &item.url, ua: &ua, fetch_mode: &mode, proxy, source_id: &source_id, ttl_minutes: ttl,
            mode: cache_mode,
            kind: SourceKind::Nodes,
        }).await.inspect_err(|error| {
            stages.push(json!({"type":"fetch","status":"error","sourceId":source_id,"message":error.message()}));
        })?;
        stages.push(json!({
            "type":"fetch","status":"ok","sourceId":source_id,
            "cached":matches!(loaded.cache_state, "fresh" | "stale"),
            "cacheState":loaded.cache_state,"message":loaded.warning.clone(),
        }));
        if let Some(message) = loaded.warning {
            diagnostics.push(Diagnostic {
                level: "warning".into(),
                source_id: Some(source_id.clone()),
                message,
            });
        }
        snapshots.push(SourceSnapshot {
            source_id: source_id.clone(),
            content_hash: format!("{:x}", Sha256::digest(loaded.content.as_bytes())),
            content: loaded.content,
        });
        let mut extra = Map::new();
        extra.insert("cache_ttl_minutes".into(), json!(ttl));
        extra.insert("fetch_mode".into(), json!(mode));
        profile.sources.push(Source {
            id: source_id,
            kind: "url".into(),
            enabled: true,
            url: item.url,
            remark: item.remark,
            prefix: item.prefix,
            content: String::new(),
            user_agent: ua,
            extra,
        });
    }
    Ok(())
}

fn source_items(fields: &SubscriptionFields) -> Result<Vec<SourceItem>, ApiError> {
    let mut items: Vec<SourceItem> = if let Some(items) = &fields.subscribe_items {
        serde_json::from_value(items.clone())
            .map_err(|error| ApiError::bad_request(format!("invalid subscribeItems: {error}")))?
    } else {
        Vec::new()
    };
    if items.is_empty() {
        let legacy = fields.subscribe_url.as_deref().unwrap_or_default().trim();
        let urls = if legacy.is_empty() {
            Vec::new()
        } else if legacy.starts_with('[') {
            serde_json::from_str::<Vec<String>>(legacy).unwrap_or_else(|_| vec![legacy.to_owned()])
        } else {
            vec![legacy.to_owned()]
        };
        items = urls
            .into_iter()
            .filter(|url| !url.is_empty())
            .map(|url| SourceItem {
                enabled: true,
                url,
                prefix: String::new(),
                remark: String::new(),
                cache_ttl_minutes: None,
                fetch_ua: None,
                fetch_mode: None,
            })
            .collect();
    }
    Ok(items)
}

pub(crate) fn source_proxy<'a>(
    state: &'a AppState,
    mode: &str,
) -> Result<Option<&'a str>, ApiError> {
    match mode {
        "auto" => Ok(None),
        "domestic-direct" => state
            .config
            .direct_proxy_url
            .as_deref()
            .map(Some)
            .ok_or_else(|| ApiError::bad_request("domestic-direct fetch proxy is not configured")),
        _ => Err(ApiError::bad_request("invalid fetchMode")),
    }
}
