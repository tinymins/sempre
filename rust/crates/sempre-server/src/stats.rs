use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::Row as _;
use uuid::Uuid;

use crate::{AppState, auth::CurrentUser, error::ApiError};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/profiles/{id}/stats", get(profile_stats))
        .route("/api/v1/stats", get(user_stats))
}

#[derive(Debug, Serialize)]
struct ProfileStats {
    total_accesses: i64,
    today_accesses: i64,
    last_access_at: Option<DateTime<Utc>>,
    by_target: Vec<TargetCount>,
    recent_accesses: Vec<AccessOutput>,
}

#[derive(Debug, Serialize)]
struct TargetCount {
    target: String,
    count: i64,
}

#[derive(Debug, Serialize)]
struct AccessOutput {
    target: String,
    user_agent: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct UserStats {
    total_profiles: i64,
    total_nodes: i64,
    today_requests: i64,
}

async fn profile_stats(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ProfileStats>, ApiError> {
    let allowed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM profiles p LEFT JOIN profile_members pm ON pm.profile_id = p.id AND pm.user_id = $2 WHERE p.id = $1 AND (p.owner_id = $2 OR pm.user_id = $2))")
        .bind(id).bind(user.id).fetch_one(&state.pool).await?;
    if !allowed {
        return Err(ApiError::not_found("profile"));
    }
    let total_accesses =
        sqlx::query_scalar("SELECT COUNT(*) FROM access_logs WHERE profile_id = $1")
            .bind(id)
            .fetch_one(&state.pool)
            .await?;
    let today_accesses = sqlx::query_scalar(
        "SELECT COUNT(*) FROM access_logs WHERE profile_id = $1 AND created_at >= CURRENT_DATE",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    let last_access_at =
        sqlx::query_scalar("SELECT MAX(created_at) FROM access_logs WHERE profile_id = $1")
            .bind(id)
            .fetch_one(&state.pool)
            .await?;
    let target_rows = sqlx::query("SELECT target, COUNT(*) AS count FROM access_logs WHERE profile_id = $1 GROUP BY target ORDER BY count DESC")
        .bind(id).fetch_all(&state.pool).await?;
    let recent_rows = sqlx::query("SELECT target, user_agent, created_at FROM access_logs WHERE profile_id = $1 ORDER BY created_at DESC LIMIT 50")
        .bind(id).fetch_all(&state.pool).await?;
    Ok(Json(ProfileStats {
        total_accesses,
        today_accesses,
        last_access_at,
        by_target: target_rows
            .iter()
            .map(target_count)
            .collect::<Result<_, _>>()?,
        recent_accesses: recent_rows
            .iter()
            .map(access_output)
            .collect::<Result<_, _>>()?,
    }))
}

async fn user_stats(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<UserStats>, ApiError> {
    let total_profiles = sqlx::query_scalar("SELECT COUNT(*) FROM profiles p LEFT JOIN profile_members pm ON pm.profile_id = p.id AND pm.user_id = $1 WHERE p.owner_id = $1 OR pm.user_id = $1")
        .bind(user.id).fetch_one(&state.pool).await?;
    let total_nodes = sqlx::query_scalar("SELECT COALESCE(SUM(a.node_count), 0) FROM artifacts a JOIN profiles p ON p.id = a.profile_id LEFT JOIN profile_members pm ON pm.profile_id = p.id AND pm.user_id = $1 WHERE (p.owner_id = $1 OR pm.user_id = $1) AND a.revision = p.revision")
        .bind(user.id).fetch_one(&state.pool).await?;
    let today_requests = sqlx::query_scalar("SELECT COUNT(*) FROM access_logs l JOIN profiles p ON p.id = l.profile_id LEFT JOIN profile_members pm ON pm.profile_id = p.id AND pm.user_id = $1 WHERE (p.owner_id = $1 OR pm.user_id = $1) AND l.created_at >= CURRENT_DATE")
        .bind(user.id).fetch_one(&state.pool).await?;
    Ok(Json(UserStats {
        total_profiles,
        total_nodes,
        today_requests,
    }))
}

fn target_count(row: &sqlx::postgres::PgRow) -> Result<TargetCount, ApiError> {
    Ok(TargetCount {
        target: row.try_get("target").map_err(ApiError::internal)?,
        count: row.try_get("count").map_err(ApiError::internal)?,
    })
}

fn access_output(row: &sqlx::postgres::PgRow) -> Result<AccessOutput, ApiError> {
    Ok(AccessOutput {
        target: row.try_get("target").map_err(ApiError::internal)?,
        user_agent: row.try_get("user_agent").map_err(ApiError::internal)?,
        created_at: row.try_get("created_at").map_err(ApiError::internal)?,
    })
}
