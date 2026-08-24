use std::sync::Arc;

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{Response, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row as _;
use uuid::Uuid;

use sempre_converter::{Target, available_targets};

use crate::{AppState, auth::token_hash, error::ApiError};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/public/subscriptions/{token}", get(manifest))
        .route(
            "/api/v1/public/subscriptions/{token}/artifacts/{target}",
            get(artifact),
        )
        .route("/api/public/proxy/{token}/{target}", get(artifact))
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
    edit_url: String,
    read_only: bool,
    available_targets: Vec<Target>,
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
    profile_id: Uuid,
    name: String,
    revision: i64,
    updated_at: DateTime<Utc>,
    content: String,
    hash: String,
    node_count: i32,
    created_at: DateTime<Utc>,
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
    let artifact_url = state
        .config
        .public_url
        .join(&format!(
            "api/v1/public/subscriptions/{token}/artifacts/{target}"
        ))
        .map_err(ApiError::internal)?
        .to_string();
    let edit_url = state
        .config
        .public_url
        .join(&format!("subscriptions/{}", value.profile_id))
        .map_err(ApiError::internal)?
        .to_string();
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
) -> Result<Response<Body>, ApiError> {
    Target::parse(&target).map_err(|error| ApiError::bad_request(error.to_string()))?;
    let value = shared_artifact(&state, &token, &target).await?;
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
    let row = sqlx::query("SELECT p.id AS profile_id, p.name, p.revision, p.updated_at, a.content, a.content_hash, a.node_count, a.created_at FROM shares s JOIN profiles p ON p.id = s.profile_id JOIN artifacts a ON a.profile_id = p.id AND a.revision = p.revision AND a.target = $2 WHERE s.token_hash = $1 AND s.enabled = TRUE AND s.revoked_at IS NULL")
        .bind(token_hash(token)).bind(target).fetch_optional(&state.pool).await?;
    let Some(row) = row else {
        return Err(ApiError::not_found("subscription artifact"));
    };
    Ok(SharedArtifact {
        profile_id: row.try_get("profile_id").map_err(ApiError::internal)?,
        name: row.try_get("name").map_err(ApiError::internal)?,
        revision: row.try_get("revision").map_err(ApiError::internal)?,
        updated_at: row.try_get("updated_at").map_err(ApiError::internal)?,
        content: row.try_get("content").map_err(ApiError::internal)?,
        hash: row.try_get("content_hash").map_err(ApiError::internal)?,
        node_count: row.try_get("node_count").map_err(ApiError::internal)?,
        created_at: row.try_get("created_at").map_err(ApiError::internal)?,
    })
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
    use super::content_type;
    #[test]
    fn selects_artifact_content_type() {
        assert!(content_type("clash-meta").starts_with("application/yaml"));
        assert!(content_type("sing-box-v13").starts_with("application/json"));
        assert!(content_type("dae").starts_with("text/plain"));
    }
}
