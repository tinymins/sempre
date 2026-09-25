use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use sqlx::Row as _;
use uuid::Uuid;

use sempre_converter::{parse_subscription, rule_provider_has_rules};

use crate::{AppState, error::ApiError, fetch};

#[derive(Clone, Copy)]
pub(crate) enum CacheMode {
    Subscription(Uuid),
    ReadOnlySubscription(Uuid),
    Global,
    ReadOnlyGlobal,
    Bypass,
}

#[derive(Clone, Copy)]
pub(crate) enum SourceKind {
    Nodes,
    RuleSet,
}

pub(crate) struct LoadedSource {
    pub content: String,
    pub usable: bool,
    pub cache_state: &'static str,
    pub warning: Option<String>,
    pub http_status: Option<u16>,
    pub response_headers: BTreeMap<String, String>,
}

pub(crate) struct SourceRequest<'a> {
    pub url: &'a str,
    pub ua: &'a str,
    pub fetch_mode: &'a str,
    pub proxy: Option<&'a str>,
    pub source_id: &'a str,
    pub ttl_minutes: i32,
    pub mode: CacheMode,
    pub kind: SourceKind,
    pub inspect_unusable: bool,
}

fn cache_key(request: &SourceRequest<'_>) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}\0{}\0{}\0{}",
                request.url, request.ua, request.fetch_mode, request.source_id
            )
            .as_bytes()
        )
    )
}

pub(crate) async fn load(
    state: &AppState,
    request: SourceRequest<'_>,
) -> Result<LoadedSource, ApiError> {
    if request.ttl_minutes < 0 {
        return Err(ApiError::bad_request("cache TTL must be nonnegative"));
    }
    let cache_key = cache_key(&request);
    let cached = match request.mode {
        CacheMode::Subscription(id) | CacheMode::ReadOnlySubscription(id) => {
            sqlx::query("SELECT content,fetched_at >= NOW() - ($3 * INTERVAL '1 minute') AS fresh FROM subscription_source_snapshots WHERE subscribe_id=$1 AND source_id=$2")
                .bind(id).bind(&cache_key).bind(request.ttl_minutes)
                .fetch_optional(&state.pool).await?
        }
        CacheMode::Global | CacheMode::ReadOnlyGlobal => {
            sqlx::query("SELECT content,fetched_at >= NOW() - ($2 * INTERVAL '1 minute') AS fresh FROM source_debug_cache WHERE cache_key=$1")
                .bind(&cache_key).bind(request.ttl_minutes)
                .fetch_optional(&state.pool).await?
        }
        CacheMode::Bypass => None,
    };
    if request.ttl_minutes > 0
        && let Some(row) = &cached
    {
        let fresh: bool = row.try_get("fresh").map_err(ApiError::internal)?;
        if fresh {
            return Ok(LoadedSource {
                content: row.try_get("content").map_err(ApiError::internal)?,
                usable: true,
                cache_state: "fresh",
                warning: None,
                http_status: None,
                response_headers: BTreeMap::new(),
            });
        }
    }
    let fetched = fetch::fetch_source_text(request.url, request.ua, request.proxy).await;
    let usable = fetched.as_ref().is_ok_and(|fetched| {
        fetched.status == 200
            && match request.kind {
                SourceKind::Nodes => !parse_subscription(&fetched.content).nodes.is_empty(),
                SourceKind::RuleSet => rule_provider_has_rules(&fetched.content),
            }
    });
    if usable {
        let fetched = fetched.expect("usable fetch is successful");
        let content = fetched.content;
        let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
        match request.mode {
            CacheMode::Subscription(id) => {
                sqlx::query("INSERT INTO subscription_source_snapshots (subscribe_id,source_id,content,content_hash,fetched_at,last_status) VALUES ($1,$2,$3,$4,NOW(),'ok') ON CONFLICT (subscribe_id,source_id) DO UPDATE SET content=EXCLUDED.content,content_hash=EXCLUDED.content_hash,fetched_at=EXCLUDED.fetched_at,last_status='ok',last_error=NULL")
                    .bind(id).bind(&cache_key).bind(&content).bind(&hash).execute(&state.pool).await?;
            }
            CacheMode::Global => {
                sqlx::query("INSERT INTO source_debug_cache (cache_key,content,content_hash,fetched_at) VALUES ($1,$2,$3,NOW()) ON CONFLICT (cache_key) DO UPDATE SET content=EXCLUDED.content,content_hash=EXCLUDED.content_hash,fetched_at=EXCLUDED.fetched_at")
                    .bind(&cache_key).bind(&content).bind(&hash).execute(&state.pool).await?;
            }
            CacheMode::ReadOnlySubscription(_) | CacheMode::ReadOnlyGlobal | CacheMode::Bypass => {}
        }
        return Ok(LoadedSource {
            content,
            usable: true,
            cache_state: if matches!(request.mode, CacheMode::Bypass) {
                "bypass"
            } else {
                "miss"
            },
            warning: None,
            http_status: Some(fetched.status),
            response_headers: fetched.headers,
        });
    }
    if let Some(row) = cached {
        let warning = match fetched {
            Ok(response) if response.status != 200 => format!(
                "refresh returned HTTP {}; using stale snapshot",
                response.status
            ),
            Ok(_) => "refresh returned no usable entries; using stale snapshot".to_owned(),
            Err(error) => format!("refresh failed ({}); using stale snapshot", error.message()),
        };
        if let CacheMode::Subscription(id) = request.mode {
            sqlx::query("UPDATE subscription_source_snapshots SET last_status='error',last_error=$3 WHERE subscribe_id=$1 AND source_id=$2")
                .bind(id).bind(&cache_key).bind(&warning).execute(&state.pool).await?;
        }
        return Ok(LoadedSource {
            content: row.try_get("content").map_err(ApiError::internal)?,
            usable: true,
            cache_state: "stale",
            warning: Some(warning),
            http_status: None,
            response_headers: BTreeMap::new(),
        });
    }
    unusable_result(&request, fetched)
}

fn unusable_result(
    request: &SourceRequest<'_>,
    fetched: Result<fetch::FetchedText, ApiError>,
) -> Result<LoadedSource, ApiError> {
    match fetched {
        Ok(response) if request.inspect_unusable => Ok(LoadedSource {
            content: response.content,
            usable: false,
            cache_state: if matches!(request.mode, CacheMode::Bypass) {
                "bypass"
            } else {
                "miss"
            },
            warning: Some(if response.status == 200 {
                "source returned no usable entries".into()
            } else {
                format!("source returned HTTP {}", response.status)
            }),
            http_status: Some(response.status),
            response_headers: response.headers,
        }),
        Ok(response) if response.status != 200 => Err(ApiError::unavailable(format!(
            "source returned HTTP {}",
            response.status
        ))),
        Ok(_) => Err(ApiError::unavailable(match request.kind {
            SourceKind::Nodes => "source has no usable nodes",
            SourceKind::RuleSet => "rule provider has no usable rules",
        })),
        Err(error) => Err(error),
    }
}
