use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row as _;
use uuid::Uuid;

use sempre_converter::{CustomNode, Proxy};

use crate::{AppState, auth::CurrentUser, error::ApiError};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/custom-nodes", get(list).post(create))
        .route(
            "/api/v1/custom-nodes/{id}",
            get(get_node).put(update).delete(remove),
        )
}

#[derive(Debug, Deserialize)]
struct CustomNodeInput {
    name: String,
    proxy: Value,
    #[serde(default)]
    authorized_user_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
struct CustomNodeOutput {
    id: Uuid,
    owner_id: Uuid,
    name: String,
    proxy: Value,
    authorized_user_ids: Vec<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

async fn list(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<CustomNodeOutput>>, ApiError> {
    let rows = sqlx::query("SELECT id, owner_id, name, proxy, authorized_user_ids, created_at, updated_at FROM custom_nodes WHERE owner_id = $1 OR $1 = ANY(authorized_user_ids) ORDER BY updated_at DESC")
        .bind(user.id).fetch_all(&state.pool).await?;
    rows.iter()
        .map(output)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

async fn create(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<CustomNodeInput>,
) -> Result<(StatusCode, Json<CustomNodeOutput>), ApiError> {
    let (name, proxy, authorized) = validate_input(&state, input).await?;
    let row = sqlx::query("INSERT INTO custom_nodes (id, owner_id, name, proxy, authorized_user_ids) VALUES ($1, $2, $3, $4, $5) RETURNING id, owner_id, name, proxy, authorized_user_ids, created_at, updated_at")
        .bind(Uuid::new_v4()).bind(user.id).bind(name).bind(proxy).bind(authorized)
        .fetch_one(&state.pool).await?;
    Ok((StatusCode::CREATED, Json(output(&row)?)))
}

async fn get_node(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<CustomNodeOutput>, ApiError> {
    let row = visible_row(&state, id, user.id)
        .await?
        .ok_or_else(|| ApiError::not_found("custom node"))?;
    output(&row).map(Json)
}

async fn update(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<CustomNodeInput>,
) -> Result<Json<CustomNodeOutput>, ApiError> {
    if visible_row(&state, id, user.id).await?.is_none() {
        return Err(ApiError::not_found("custom node"));
    }
    let (name, proxy, authorized) = validate_input(&state, input).await?;
    let row = sqlx::query("UPDATE custom_nodes SET name = $1, proxy = $2, authorized_user_ids = $3, updated_at = NOW() WHERE id = $4 RETURNING id, owner_id, name, proxy, authorized_user_ids, created_at, updated_at")
        .bind(name).bind(proxy).bind(authorized).bind(id).fetch_one(&state.pool).await?;
    output(&row).map(Json)
}

async fn remove(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("DELETE FROM custom_nodes WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("custom node"));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn load_selected(
    state: &AppState,
    profile_owner: Uuid,
    ids: &[String],
) -> Result<Vec<CustomNode>, ApiError> {
    let ids = ids
        .iter()
        .map(|id| Uuid::parse_str(id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ApiError::bad_request("profile contains an invalid custom node ID"))?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query("SELECT id, name, proxy FROM custom_nodes WHERE id = ANY($1) AND (owner_id = $2 OR $2 = ANY(authorized_user_ids))")
        .bind(&ids).bind(profile_owner).fetch_all(&state.pool).await?;
    if rows.len() != ids.len() {
        return Err(ApiError::bad_request(
            "profile references an unavailable custom node",
        ));
    }
    rows.iter()
        .map(|row| {
            Ok(CustomNode {
                id: row
                    .try_get::<Uuid, _>("id")
                    .map_err(ApiError::internal)?
                    .to_string(),
                name: row.try_get("name").map_err(ApiError::internal)?,
                proxy: row.try_get("proxy").map_err(ApiError::internal)?,
                created_at: None,
                updated_at: None,
            })
        })
        .collect()
}

async fn validate_input(
    state: &AppState,
    input: CustomNodeInput,
) -> Result<(String, Value, Vec<Uuid>), ApiError> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(ApiError::bad_request(
            "custom node name must be between 1 and 100 characters",
        ));
    }
    Proxy::from_value(input.proxy.clone())
        .map_err(|error| ApiError::bad_request(format!("invalid custom node: {error}")))?;
    let mut authorized = input.authorized_user_ids;
    authorized.sort_unstable();
    authorized.dedup();
    if !authorized.is_empty() {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = ANY($1)")
            .bind(&authorized)
            .fetch_one(&state.pool)
            .await?;
        if usize::try_from(count).ok() != Some(authorized.len()) {
            return Err(ApiError::bad_request("authorized user does not exist"));
        }
    }
    Ok((name.into(), input.proxy, authorized))
}

async fn visible_row(
    state: &AppState,
    id: Uuid,
    user_id: Uuid,
) -> Result<Option<sqlx::postgres::PgRow>, ApiError> {
    sqlx::query("SELECT id, owner_id, name, proxy, authorized_user_ids, created_at, updated_at FROM custom_nodes WHERE id = $1 AND (owner_id = $2 OR $2 = ANY(authorized_user_ids))")
        .bind(id).bind(user_id).fetch_optional(&state.pool).await.map_err(Into::into)
}

fn output(row: &sqlx::postgres::PgRow) -> Result<CustomNodeOutput, ApiError> {
    Ok(CustomNodeOutput {
        id: row.try_get("id").map_err(ApiError::internal)?,
        owner_id: row.try_get("owner_id").map_err(ApiError::internal)?,
        name: row.try_get("name").map_err(ApiError::internal)?,
        proxy: row.try_get("proxy").map_err(ApiError::internal)?,
        authorized_user_ids: row
            .try_get("authorized_user_ids")
            .map_err(ApiError::internal)?,
        created_at: row.try_get("created_at").map_err(ApiError::internal)?,
        updated_at: row.try_get("updated_at").map_err(ApiError::internal)?,
    })
}
