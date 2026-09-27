use sempre_converter::{Diagnostic, Profile, Source, SourceSnapshot, parse_subscription};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::{
    AppState,
    debug_stream::StageLog,
    error::ApiError,
    source_cache::{self, CacheMode, SourceKind, SourceRequest},
    subscriptions::SubscriptionFields,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceItem {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub url: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    remark: String,
    pub cache_ttl_minutes: Option<i32>,
    pub fetch_ua: Option<String>,
    pub fetch_mode: Option<String>,
}

impl SourceItem {
    pub(crate) fn effective_ua(&self) -> &str {
        self.fetch_ua
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("clash.meta")
    }
}

fn default_true() -> bool {
    true
}

pub(crate) struct SourceLoadSummary {
    pub enabled: usize,
    pub failed: usize,
}

pub(crate) async fn load_sources(
    state: &AppState,
    fields: &SubscriptionFields,
    profile: &mut Profile,
    cache_mode: CacheMode,
    stages: &mut StageLog,
    snapshots: &mut Vec<SourceSnapshot>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<SourceLoadSummary, ApiError> {
    let items = source_items(fields)?;
    let enabled_items = items
        .into_iter()
        .filter(|item| item.enabled)
        .collect::<Vec<_>>();
    let mut summary = SourceLoadSummary {
        enabled: enabled_items.len(),
        failed: 0,
    };
    if enabled_items.is_empty() {
        stages.push(
            json!({"type":"fetch","status":"skipped","message":"no enabled subscription sources"}),
        );
    }
    for item in enabled_items {
        if item.url.trim().is_empty() {
            return Err(ApiError::bad_request("enabled source URL is empty"));
        }
        let ua = item.effective_ua().to_owned();
        let mode = item.fetch_mode.unwrap_or_else(|| "auto".into());
        let proxy = source_proxy(state, &mode)?;
        let source_id = source_id(&item.url, &ua, &mode);
        stages.push(json!({"type":"fetch","status":"running","sourceId":source_id}));
        let ttl = item
            .cache_ttl_minutes
            .or(fields.cache_ttl_minutes)
            .unwrap_or(60);
        if ttl < 0 {
            return Err(ApiError::bad_request("cache TTL must be nonnegative"));
        }
        let loaded = source_cache::load(
            state,
            SourceRequest {
                url: &item.url,
                ua: &ua,
                fetch_mode: &mode,
                proxy,
                source_id: &source_id,
                ttl_minutes: ttl,
                mode: cache_mode,
                kind: SourceKind::Nodes,
                inspect_unusable: true,
            },
        )
        .await;
        let loaded = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                summary.failed += 1;
                stages.push(json!({"type":"fetch","status":"error","sourceId":source_id,"message":error.message()}));
                diagnostics.push(Diagnostic {
                    level: "error".into(),
                    source_id: Some(source_id),
                    message: error.message().into(),
                });
                continue;
            }
        };
        if !loaded.usable {
            summary.failed += 1;
            record_unusable(loaded, &source_id, stages, diagnostics);
            continue;
        }
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
    Ok(summary)
}

fn record_unusable(
    loaded: source_cache::LoadedSource,
    source_id: &str,
    stages: &mut StageLog,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let message = loaded
        .warning
        .unwrap_or_else(|| "source has no usable nodes".into());
    stages.push(json!({"type":"fetch","status":"error","sourceId":source_id,"httpStatus":loaded.http_status,"message":message.clone()}));
    diagnostics.push(Diagnostic {
        level: "error".into(),
        source_id: Some(source_id.into()),
        message,
    });
    if loaded.http_status == Some(200) {
        diagnostics.extend(
            parse_subscription(&loaded.content)
                .diagnostics
                .into_iter()
                .map(|message| Diagnostic {
                    level: "warning".into(),
                    source_id: Some(source_id.into()),
                    message,
                }),
        );
    }
}

pub(crate) fn source_items(fields: &SubscriptionFields) -> Result<Vec<SourceItem>, ApiError> {
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

pub(crate) fn source_id(url: &str, ua: &str, mode: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{url}\0{ua}\0{mode}").as_bytes())
    )
}

pub(crate) fn all_sources_failed(stages: &[Value]) -> bool {
    let fetches = stages
        .iter()
        .filter(|stage| stage.get("type").and_then(Value::as_str) == Some("fetch"));
    let mut failed = false;
    for stage in fetches {
        match stage.get("status").and_then(Value::as_str) {
            Some("ok") => return false,
            Some("error") => failed = true,
            _ => {}
        }
    }
    failed
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
