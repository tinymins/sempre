use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, post},
};
use sempre_converter::{Profile, Target, preview_nodes as inspect_nodes, trace_node_steps};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row as _;
use uuid::Uuid;

use crate::{
    AppState,
    auth::CurrentUser,
    error::ApiError,
    fetch::{self, SourceTestResult},
    profiles, publishing,
};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/profiles/{id}/preview-nodes", post(preview_nodes))
        .route("/api/v1/profiles/{id}/trace-node", post(trace_node))
        .route(
            "/api/v1/profiles/{id}/sources/{source_id}/test",
            post(test_source),
        )
        .route(
            "/api/v1/profiles/{id}/sources/{source_id}/cache",
            delete(clear_cache),
        )
}

#[derive(Debug, Deserialize)]
struct TargetInput {
    target: Target,
}

#[derive(Debug, Deserialize)]
struct TraceInput {
    target: Target,
    name: String,
}

async fn preview_nodes(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<TargetInput>,
) -> Result<Json<Value>, ApiError> {
    profiles::require_read(&state, id, &user).await?;
    let (_, request) = publishing::compile_request(&state, id, input.target).await?;
    let nodes =
        inspect_nodes(&request).map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(json!({ "nodes": nodes })))
}

async fn trace_node(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<TraceInput>,
) -> Result<Json<Value>, ApiError> {
    profiles::require_read(&state, id, &user).await?;
    let (_, request) = publishing::compile_request(&state, id, input.target).await?;
    let trace = trace_node_steps(&request, &input.name)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(trace))
}

async fn test_source(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path((id, source_id)): Path<(Uuid, String)>,
) -> Result<Json<SourceTestResult>, ApiError> {
    profiles::require_write(&state, id, &user).await?;
    let profile = load_profile(&state, id).await?;
    let source = profile
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .ok_or_else(|| ApiError::not_found("source"))?;
    fetch::test_source(&state, source).await.map(Json)
}

async fn clear_cache(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path((id, source_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, ApiError> {
    profiles::require_write(&state, id, &user).await?;
    let profile = load_profile(&state, id).await?;
    if !profile.sources.iter().any(|source| source.id == source_id) {
        return Err(ApiError::not_found("source"));
    }
    fetch::clear_snapshot(&state, id, &source_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn load_profile(state: &AppState, id: Uuid) -> Result<Profile, ApiError> {
    let row = sqlx::query("SELECT document FROM profiles WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("profile"))?;
    let document: Value = row.try_get("document").map_err(ApiError::internal)?;
    serde_json::from_value(document).map_err(ApiError::internal)
}
