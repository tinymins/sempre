use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    body::Body,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, Response, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row as _, postgres::PgRow};
use uuid::Uuid;

use sempre_converter::{Target, compile};

use crate::{
    AppState,
    error::ApiError,
    export_target::{available as available_targets, from_path as suffix_target},
    source_cache::CacheMode,
    subscription_compile::{PrepareOptions, PreparedInput, prepare_local, prepare_with_local},
    subscriptions::{SubscriptionFields, row_fields},
    trusted_proxy::client_ip,
};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/subscriptions/{url}/manifest", get(manifest))
        .route("/api/subscriptions/{url}/{*suffix}", get(direct))
        .route(
            "/api/subscriptions/{url}/artifacts/{artifact_id}",
            get(artifact),
        )
}

#[derive(Deserialize)]
struct ManifestQuery {
    target: Option<String>,
}

struct Subscription {
    id: Uuid,
    owner: Uuid,
    name: String,
    updated_at: DateTime<Utc>,
    fields: SubscriptionFields,
    selected: Vec<Uuid>,
}

struct Generated {
    subscription: Subscription,
    revision_hash: String,
    target: Target,
    content: String,
    hash: String,
    input_hash: String,
    node_count: i32,
    runtime: Value,
    stale_source: bool,
}

struct StoredArtifact {
    id: Uuid,
    subscribe_id: Uuid,
    target: String,
    content: String,
    hash: String,
    node_count: i32,
    profile_name: String,
    profile_revision: i64,
    profile_updated_at: DateTime<Utc>,
    runtime: Value,
    created_at: DateTime<Utc>,
}

impl StoredArtifact {
    fn from_row(row: &PgRow) -> Result<Self, ApiError> {
        Ok(Self {
            id: row.try_get("id").map_err(ApiError::internal)?,
            subscribe_id: row.try_get("subscribe_id").map_err(ApiError::internal)?,
            target: row.try_get("target").map_err(ApiError::internal)?,
            content: row.try_get("content").map_err(ApiError::internal)?,
            hash: row.try_get("content_hash").map_err(ApiError::internal)?,
            node_count: row.try_get("node_count").map_err(ApiError::internal)?,
            profile_name: row.try_get("profile_name").map_err(ApiError::internal)?,
            profile_revision: row
                .try_get("profile_revision")
                .map_err(ApiError::internal)?,
            profile_updated_at: row
                .try_get("profile_updated_at")
                .map_err(ApiError::internal)?,
            runtime: row.try_get("runtime").map_err(ApiError::internal)?,
            created_at: row.try_get("created_at").map_err(ApiError::internal)?,
        })
    }
}

async fn direct(
    State(state): State<Arc<AppState>>,
    Path((url, suffix)): Path<(String, String)>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response<Body>, ApiError> {
    let target = suffix_target(&suffix).ok_or_else(|| ApiError::not_found("target"))?;
    let subscription = find_subscription(&state, &url).await?;
    let (stored, is_stale) = resolve_current(&state, subscription, target).await?;
    record_access(
        &state,
        stored.subscribe_id,
        &stored.target,
        stored.node_count,
        peer,
        &headers,
    )
    .await;
    response(
        stored.content,
        &stored.target,
        &stored.hash,
        false,
        is_stale,
        true,
    )
}

async fn manifest(
    State(state): State<Arc<AppState>>,
    Path(url): Path<String>,
    Query(query): Query<ManifestQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let target_name = query.target.unwrap_or_else(|| "sing-box-v13".into());
    let mut target =
        Target::parse(&target_name).map_err(|error| ApiError::bad_request(error.to_string()))?;
    target.standalone = true;
    let subscription = find_subscription(&state, &url).await?;
    let (stored, is_stale) = resolve_current(&state, subscription, target.clone()).await?;
    let artifact_url = state
        .config
        .public_url
        .join(&format!("api/subscriptions/{url}/artifacts/{}", stored.id))
        .map_err(ApiError::internal)?;
    let mut edit_url = state.config.public_url.clone();
    edit_url.set_fragment(Some(&format!("/subscriptions/{}", stored.subscribe_id)));
    let body = json!({
        "schema": 1,
        "service": "sempre",
        "profile": {
            "name": stored.profile_name,
            "revision": stored.profile_revision,
            "updated_at": stored.profile_updated_at,
        },
        "target": target,
        "artifact": {
            "url": artifact_url.to_string(),
            "sha256": stored.hash,
            "content_type": content_type(&target_name),
            "node_count": stored.node_count,
            "created_at": stored.created_at,
        },
        "runtime": stored.runtime,
        "edit_url": edit_url.to_string(),
        "read_only": true,
        "available_targets": available_targets(),
    });
    let mut response = Json(body).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "public, max-age=60".parse().expect("static header"),
    );
    if is_stale {
        response
            .headers_mut()
            .insert("x-sempre-stale", "true".parse().expect("static header"));
    }
    Ok(response)
}

async fn artifact(
    State(state): State<Arc<AppState>>,
    Path((url, artifact_id)): Path<(String, Uuid)>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response<Body>, ApiError> {
    let row = sqlx::query("SELECT a.* FROM subscription_artifacts a JOIN proxy_subscribes s ON s.id=a.subscribe_id WHERE a.id=$1 AND s.url=$2")
        .bind(artifact_id).bind(&url).fetch_optional(&state.pool).await?
        .ok_or_else(|| ApiError::not_found("subscription artifact"))?;
    let stored = StoredArtifact::from_row(&row)?;
    record_access(
        &state,
        stored.subscribe_id,
        &stored.target,
        stored.node_count,
        peer,
        &headers,
    )
    .await;
    response(
        stored.content,
        &stored.target,
        &stored.hash,
        true,
        false,
        false,
    )
}

async fn resolve_current(
    state: &AppState,
    subscription: Subscription,
    target: Target,
) -> Result<(StoredArtifact, bool), ApiError> {
    let id = subscription.id;
    let target_name = target.format.clone();
    let mut stages = crate::debug_stream::StageLog::default();
    let local = prepare_local(
        state,
        &subscription.fields,
        &subscription.selected,
        target,
        &PrepareOptions {
            viewer: subscription.owner,
            cache_mode: CacheMode::Subscription(id),
            node_scope: Some(id),
            include_rule_snapshots: true,
        },
        &mut stages,
    )
    .await?;
    let revision = revision_hash(&subscription, &local)?;
    match generate(state, subscription, local, revision.clone(), stages).await {
        Ok(generated) => match persist_artifact(state, &generated).await {
            Ok(stored) => Ok((stored, generated.stale_source)),
            Err(error) => last_good(state, id, &target_name, &revision, error).await,
        },
        Err(error) => last_good(state, id, &target_name, &revision, error).await,
    }
}

async fn last_good(
    state: &AppState,
    id: Uuid,
    target: &str,
    revision: &str,
    error: ApiError,
) -> Result<(StoredArtifact, bool), ApiError> {
    let row = sqlx::query("SELECT * FROM subscription_artifacts WHERE subscribe_id=$1 AND target=$2 AND revision_hash=$3 ORDER BY last_success_at DESC LIMIT 1")
        .bind(id).bind(target).bind(revision).fetch_optional(&state.pool).await?;
    if let Some(row) = row {
        tracing::warn!(subscribe_id=%id, target, reason=%error.message(), "serving last known good subscription artifact");
        Ok((StoredArtifact::from_row(&row)?, true))
    } else {
        Err(error)
    }
}

async fn find_subscription(state: &AppState, url: &str) -> Result<Subscription, ApiError> {
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *transaction)
        .await?;
    let row = sqlx::query("SELECT * FROM proxy_subscribes WHERE url=$1")
        .bind(url)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| ApiError::not_found("subscription"))?;
    let id: Uuid = row.try_get("id").map_err(ApiError::internal)?;
    let fields = row_fields(&row)?;
    let selected_rows = sqlx::query("SELECT custom_node_id FROM proxy_subscribe_custom_nodes WHERE subscribe_id=$1 AND enabled=TRUE ORDER BY position")
        .bind(id).fetch_all(&mut *transaction).await?;
    let selected = selected_rows
        .iter()
        .map(|row| row.try_get("custom_node_id").map_err(ApiError::internal))
        .collect::<Result<Vec<_>, _>>()?;
    transaction.commit().await?;
    let updated_at: Option<DateTime<Utc>> =
        row.try_get("updated_at").map_err(ApiError::internal)?;
    let created_at: Option<DateTime<Utc>> =
        row.try_get("created_at").map_err(ApiError::internal)?;
    Ok(Subscription {
        id,
        owner: row.try_get("user_id").map_err(ApiError::internal)?,
        name: row
            .try_get::<Option<String>, _>("remark")
            .map_err(ApiError::internal)?
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Subscription".into()),
        updated_at: updated_at
            .or(created_at)
            .unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
        fields,
        selected,
    })
}

fn revision_hash(subscription: &Subscription, local: &PreparedInput) -> Result<String, ApiError> {
    let input = serde_json::to_vec(&(
        subscription.updated_at.timestamp_micros(),
        &local.profile,
        &local.custom_nodes,
    ))
    .map_err(ApiError::internal)?;
    Ok(format!("{:x}", Sha256::digest(input)))
}

async fn generate(
    state: &AppState,
    subscription: Subscription,
    local: PreparedInput,
    revision_hash: String,
    mut stages: crate::debug_stream::StageLog,
) -> Result<Generated, ApiError> {
    let (request, _) = prepare_with_local(
        state,
        &subscription.fields,
        PrepareOptions {
            viewer: subscription.owner,
            cache_mode: CacheMode::Subscription(subscription.id),
            node_scope: Some(subscription.id),
            include_rule_snapshots: true,
        },
        local,
        &mut stages,
    )
    .await?;
    let mut input = serde_json::to_vec(&request).map_err(ApiError::internal)?;
    input.extend_from_slice(subscription.updated_at.to_rfc3339().as_bytes());
    let input_hash = format!("{:x}", Sha256::digest(&input));
    let result = compile(&request).map_err(|error| ApiError::bad_request(error.to_string()))?;
    if result.node_count == 0 && crate::subscription_sources::all_sources_failed(&stages) {
        return Err(ApiError::unavailable(
            "all enabled subscription sources failed and no usable nodes remain",
        ));
    }
    let hash = format!("{:x}", Sha256::digest(result.content.as_bytes()));
    let runtime = json!({
        "local_proxy": request.profile.local_proxy,
        "transparent_proxy": request.profile.transparent_proxy,
        "management_api": request.profile.management_api,
    });
    let stale_source = stages.iter().any(|stage| {
        stage.get("cacheState").and_then(Value::as_str) == Some("stale")
            || (stage.get("type").and_then(Value::as_str) == Some("fetch")
                && stage.get("status").and_then(Value::as_str) == Some("error"))
    });
    Ok(Generated {
        subscription,
        revision_hash,
        target: request.target,
        content: result.content,
        hash,
        input_hash,
        node_count: i32::try_from(result.node_count).map_err(ApiError::internal)?,
        runtime,
        stale_source,
    })
}

async fn persist_artifact(
    state: &AppState,
    generated: &Generated,
) -> Result<StoredArtifact, ApiError> {
    let id = Uuid::new_v4();
    let row = sqlx::query("INSERT INTO subscription_artifacts (id,subscribe_id,target,input_hash,content,content_hash,node_count,profile_name,profile_revision,profile_updated_at,runtime,revision_hash) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT (subscribe_id,target,input_hash,content_hash) DO UPDATE SET last_success_at=GREATEST(clock_timestamp(),subscription_artifacts.last_success_at + INTERVAL '1 microsecond'),revision_hash=EXCLUDED.revision_hash RETURNING *")
        .bind(id).bind(generated.subscription.id).bind(&generated.target.format)
        .bind(&generated.input_hash).bind(&generated.content).bind(&generated.hash)
        .bind(generated.node_count).bind(&generated.subscription.name)
        .bind(generated.subscription.updated_at.timestamp_micros())
        .bind(generated.subscription.updated_at).bind(&generated.runtime).bind(&generated.revision_hash)
        .fetch_one(&state.pool).await?;
    StoredArtifact::from_row(&row)
}

async fn record_access(
    state: &AppState,
    id: Uuid,
    target: &str,
    node_count: i32,
    peer: SocketAddr,
    headers: &HeaderMap,
) {
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .chars()
        .take(512)
        .collect::<String>();
    let mut transaction = match state.pool.begin().await {
        Ok(transaction) => transaction,
        Err(error) => {
            tracing::warn!(%error, "access log unavailable");
            return;
        }
    };
    let write = sqlx::query("INSERT INTO proxy_access_logs (id,subscribe_id,access_type,ip,user_agent,node_count) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::new_v4()).bind(id).bind(target).bind(client_ip(state, peer, headers).to_string()).bind(&user_agent).bind(node_count)
        .execute(&mut *transaction).await;
    let update = sqlx::query(
        "UPDATE proxy_subscribes SET cached_node_count=$1,last_access_at=NOW(),access_total=access_total+1 WHERE id=$2",
    )
    .bind(node_count)
    .bind(id)
    .execute(&mut *transaction)
    .await;
    if let (Ok(_), Ok(_)) = (write, update) {
        if let Err(error) = transaction.commit().await {
            tracing::warn!(%error, "access log commit failed");
        }
    } else {
        tracing::warn!(subscribe_id=%id, "access log write failed");
    }
}

fn response(
    content: String,
    target: &str,
    hash: &str,
    immutable: bool,
    stale: bool,
    inline_text: bool,
) -> Result<Response<Body>, ApiError> {
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            if inline_text {
                "text/plain; charset=utf-8"
            } else {
                content_type(target)
            },
        )
        .header(header::ETAG, format!("\"{hash}\""))
        .header(
            header::CACHE_CONTROL,
            if immutable {
                "public, max-age=31536000, immutable"
            } else {
                "public, max-age=60"
            },
        );
    if inline_text {
        builder = builder.header(header::CONTENT_DISPOSITION, "inline");
    }
    if stale {
        builder = builder
            .header("x-sempre-stale", "true")
            .header("warning", "110 - \"stale configuration\"");
    }
    builder
        .body(Body::from(content))
        .map_err(ApiError::internal)
}

fn content_type(target: &str) -> &'static str {
    if target == "dae" {
        "text/plain; charset=utf-8"
    } else if matches!(target, "clash" | "clash-meta" | "clash-rs") {
        "application/yaml; charset=utf-8"
    } else {
        "application/json; charset=utf-8"
    }
}
