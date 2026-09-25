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
    pub cache_state: &'static str,
    pub warning: Option<String>,
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
}

pub(crate) async fn load(
    state: &AppState,
    request: SourceRequest<'_>,
) -> Result<LoadedSource, ApiError> {
    if request.ttl_minutes < 0 {
        return Err(ApiError::bad_request("cache TTL must be nonnegative"));
    }
    let cache_key = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}\0{}\0{}\0{}",
                request.url, request.ua, request.fetch_mode, request.source_id
            )
            .as_bytes()
        )
    );
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
                cache_state: "fresh",
                warning: None,
            });
        }
    }
    let fetched = fetch::fetch_source_text(request.url, request.ua, request.proxy).await;
    let usable = fetched.as_ref().is_ok_and(|content| match request.kind {
        SourceKind::Nodes => !parse_subscription(content).nodes.is_empty(),
        SourceKind::RuleSet => rule_provider_has_rules(content),
    });
    if usable {
        let content = fetched.expect("usable fetch is successful");
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
            cache_state: if matches!(request.mode, CacheMode::Bypass) {
                "bypass"
            } else {
                "miss"
            },
            warning: None,
        });
    }
    if let Some(row) = cached {
        let warning = match fetched {
            Ok(_) => "refresh returned no usable entries; using stale snapshot".to_owned(),
            Err(error) => format!("refresh failed ({}); using stale snapshot", error.message()),
        };
        if let CacheMode::Subscription(id) = request.mode {
            sqlx::query("UPDATE subscription_source_snapshots SET last_status='error',last_error=$3 WHERE subscribe_id=$1 AND source_id=$2")
                .bind(id).bind(&cache_key).bind(&warning).execute(&state.pool).await?;
        }
        return Ok(LoadedSource {
            content: row.try_get("content").map_err(ApiError::internal)?,
            cache_state: "stale",
            warning: Some(warning),
        });
    }
    match fetched {
        Ok(_) => Err(ApiError::unavailable(match request.kind {
            SourceKind::Nodes => "source has no usable nodes",
            SourceKind::RuleSet => "rule provider has no usable rules",
        })),
        Err(error) => Err(error),
    }
}
