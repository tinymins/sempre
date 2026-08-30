use std::sync::Arc;

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, Response, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row as _;
use uuid::Uuid;

use sempre_converter::{Target, available_targets};

use crate::{AppState, auth::token_hash, error::ApiError, fetch};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/public/subscriptions/{token}", get(manifest))
        .route(
            "/api/v1/public/subscriptions/{token}/artifacts/{target}",
            get(artifact),
        )
        .route("/api/public/proxy/{token}/{target}", get(artifact))
        .route("/api/v1/public/rules/sing-box", get(rule_set_v1))
        .route("/api/v1/public/rules/sing-box/12", get(rule_set_v12))
        .route("/api/v1/public/rules/sing-box/13", get(rule_set_v13))
        .route("/api/proxy/sing-box/convert/rule", get(rule_set_v1))
        .route("/api/proxy/sing-box/convert/rule/12", get(rule_set_v12))
        .route("/api/proxy/sing-box/convert/rule/13", get(rule_set_v13))
}

#[derive(Debug, Deserialize)]
struct RuleSetQuery {
    url: String,
}

async fn rule_set_v1(query: Query<RuleSetQuery>) -> Result<Json<Value>, ApiError> {
    convert_rule_set(query, 1).await
}

async fn rule_set_v12(query: Query<RuleSetQuery>) -> Result<Json<Value>, ApiError> {
    convert_rule_set(query, 3).await
}

async fn rule_set_v13(query: Query<RuleSetQuery>) -> Result<Json<Value>, ApiError> {
    convert_rule_set(query, 4).await
}

async fn convert_rule_set(
    Query(query): Query<RuleSetQuery>,
    version: u8,
) -> Result<Json<Value>, ApiError> {
    if query.url.trim().is_empty() {
        return Err(ApiError::bad_request("url is required"));
    }
    let content = fetch::fetch_public_text(&query.url, "sempre-rule-set/1", 8 << 20).await?;
    Ok(Json(sempre_converter::convert_clash_rule_set(
        &content, version,
    )))
}

#[derive(Debug, Deserialize)]
struct ManifestQuery {
    target: Option<String>,
}

#[derive(Debug, Serialize)]
struct Manifest {
    schema: u32,
    service: &'static str,
    profile: ManifestProfile,
    target: Target,
    artifact: ManifestArtifact,
    runtime: ManifestRuntime,
    edit_url: String,
    read_only: bool,
    available_targets: Vec<Target>,
}

#[derive(Debug, Serialize)]
struct ManifestRuntime {
    local_proxy: Value,
    transparent_proxy: Value,
    management_api: Value,
}

#[derive(Debug, Serialize)]
struct ManifestProfile {
    name: String,
    revision: i64,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ManifestArtifact {
    url: String,
    sha256: String,
    content_type: &'static str,
    node_count: i32,
    created_at: DateTime<Utc>,
}

struct SharedArtifact {
    share_id: Uuid,
    profile_id: Uuid,
    name: String,
    revision: i64,
    updated_at: DateTime<Utc>,
    content: String,
    hash: String,
    node_count: i32,
    created_at: DateTime<Utc>,
    document: Value,
}

async fn manifest(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
    Query(query): Query<ManifestQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let target = query.target.unwrap_or_else(|| "sing-box-v13".into());
    let parsed_target =
        Target::parse(&target).map_err(|error| ApiError::bad_request(error.to_string()))?;
    let value = shared_artifact(&state, &token, &target).await?;
    let runtime = manifest_runtime(&value.document);
    let artifact_url = state
        .config
        .public_url
        .join(&format!(
            "api/v1/public/subscriptions/{token}/artifacts/{target}"
        ))
        .map_err(ApiError::internal)?
        .to_string();
    let edit_url = editor_url(&state.config.public_url, value.profile_id);
    let body = Manifest {
        schema: 1,
        service: "sempre",
        profile: ManifestProfile {
            name: value.name,
            revision: value.revision,
            updated_at: value.updated_at,
        },
        target: parsed_target,
        artifact: ManifestArtifact {
            url: artifact_url,
            sha256: value.hash.clone(),
            content_type: content_type(&target),
            node_count: value.node_count,
            created_at: value.created_at,
        },
        runtime,
        edit_url,
        read_only: true,
        available_targets: available_targets(),
    };
    let etag = format!("\"{}\"", value.hash);
    let mut response = Json(body).into_response();
    response
        .headers_mut()
        .insert(header::ETAG, etag.parse().map_err(ApiError::internal)?);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "private, max-age=60".parse().expect("static header"),
    );
    Ok(response)
}

async fn artifact(
    State(state): State<Arc<AppState>>,
    Path((token, target)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response<Body>, ApiError> {
    Target::parse(&target).map_err(|error| ApiError::bad_request(error.to_string()))?;
    let value = shared_artifact(&state, &token, &target).await?;
    record_access(&state, &value, &target, &headers).await;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type(&target))
        .header(header::ETAG, format!("\"{}\"", value.hash))
        .header(header::CACHE_CONTROL, "private, max-age=60")
        .body(Body::from(value.content))
        .map_err(ApiError::internal)
}

async fn shared_artifact(
    state: &AppState,
    token: &str,
    target: &str,
) -> Result<SharedArtifact, ApiError> {
    if token.len() < 32 || token.len() > 128 {
        return Err(ApiError::not_found("subscription"));
    }
    let row = sqlx::query("SELECT s.id AS share_id, p.id AS profile_id, r.name, r.revision, r.created_at AS updated_at, r.document, a.content, a.content_hash, a.node_count, a.created_at FROM shares s JOIN profiles p ON p.id = s.profile_id JOIN artifacts a ON a.profile_id = p.id AND a.target = $2 JOIN profile_revisions r ON r.profile_id = a.profile_id AND r.revision = a.revision WHERE s.token_hash = $1 AND s.enabled = TRUE AND s.revoked_at IS NULL ORDER BY a.revision DESC LIMIT 1")
        .bind(token_hash(token)).bind(target).fetch_optional(&state.pool).await?;
    let Some(row) = row else {
        return Err(ApiError::not_found("subscription artifact"));
    };
    Ok(SharedArtifact {
        share_id: row.try_get("share_id").map_err(ApiError::internal)?,
        profile_id: row.try_get("profile_id").map_err(ApiError::internal)?,
        name: row.try_get("name").map_err(ApiError::internal)?,
        revision: row.try_get("revision").map_err(ApiError::internal)?,
        updated_at: row.try_get("updated_at").map_err(ApiError::internal)?,
        content: row.try_get("content").map_err(ApiError::internal)?,
        hash: row.try_get("content_hash").map_err(ApiError::internal)?,
        node_count: row.try_get("node_count").map_err(ApiError::internal)?,
        created_at: row.try_get("created_at").map_err(ApiError::internal)?,
        document: row.try_get("document").map_err(ApiError::internal)?,
    })
}

async fn record_access(
    state: &AppState,
    artifact: &SharedArtifact,
    target: &str,
    headers: &HeaderMap,
) {
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .chars()
        .take(512)
        .collect::<String>();
    if let Err(error) = sqlx::query("INSERT INTO access_logs (share_id, profile_id, target, user_agent) VALUES ($1, $2, $3, $4)")
        .bind(artifact.share_id).bind(artifact.profile_id).bind(target).bind(user_agent)
        .execute(&state.pool).await
    {
        tracing::warn!(profile_id = %artifact.profile_id, error = %error, "failed to record subscription access");
    }
}

fn manifest_runtime(document: &Value) -> ManifestRuntime {
    ManifestRuntime {
        local_proxy: document
            .get("local_proxy")
            .cloned()
            .unwrap_or_else(|| json!({})),
        transparent_proxy: document
            .get("transparent_proxy")
            .cloned()
            .unwrap_or_else(|| json!({})),
        management_api: document
            .get("management_api")
            .cloned()
            .unwrap_or_else(|| json!({})),
    }
}

fn editor_url(base: &url::Url, profile_id: Uuid) -> String {
    let mut result = base.clone();
    result.set_fragment(Some(&format!("/server/subscriptions/{profile_id}")));
    result.to_string()
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

#[cfg(test)]
mod tests {
    use serde_json::json;
    use url::Url;
    use uuid::Uuid;

    use super::{content_type, editor_url, manifest_runtime};
    #[test]
    fn selects_artifact_content_type() {
        assert!(content_type("clash-meta").starts_with("application/yaml"));
        assert!(content_type("sing-box-v13").starts_with("application/json"));
        assert!(content_type("dae").starts_with("text/plain"));
    }

    #[test]
    fn exposes_runtime_intent_and_hash_editor_route() {
        let runtime = manifest_runtime(&json!({
            "local_proxy": { "socks_port": 1080 },
            "transparent_proxy": { "mode": "tun-router" },
            "management_api": { "external_controller": "127.0.0.1:9090" }
        }));
        assert_eq!(runtime.local_proxy["socks_port"], 1080);
        assert_eq!(runtime.transparent_proxy["mode"], "tun-router");
        let id = Uuid::parse_str("9b6bc410-0d9e-4ce4-b459-61f005809e2c").expect("uuid");
        assert_eq!(
            editor_url(
                &Url::parse("https://sempre.example/base/").expect("url"),
                id
            ),
            "https://sempre.example/base/#/server/subscriptions/9b6bc410-0d9e-4ce4-b459-61f005809e2c"
        );
    }
}
