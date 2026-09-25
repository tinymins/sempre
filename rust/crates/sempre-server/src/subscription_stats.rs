use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row as _;
use uuid::Uuid;

use crate::{AppState, auth::CurrentUser, error::ApiError};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/subscriptions/{id}/stats", get(stats))
        .route(
            "/api/v1/subscriptions/clear-cache",
            axum::routing::post(clear_cache),
        )
}

#[derive(Deserialize)]
struct ClearCacheInput {
    id: Uuid,
}

async fn clear_cache(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<ClearCacheInput>,
) -> Result<Json<Value>, ApiError> {
    let owner: Option<Uuid> =
        sqlx::query_scalar("SELECT user_id FROM proxy_subscribes WHERE id=$1")
            .bind(input.id)
            .fetch_optional(&state.pool)
            .await?;
    match owner {
        None => return Err(ApiError::not_found("subscription")),
        Some(owner) if owner != user.id => {
            return Err(ApiError::forbidden("only the owner can clear cache"));
        }
        _ => {}
    }
    let cleared = sqlx::query("DELETE FROM subscription_source_snapshots WHERE subscribe_id=$1")
        .bind(input.id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    Ok(Json(json!({"cleared": cleared})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatsQuery {
    page: Option<i64>,
    page_size: Option<i64>,
}

async fn stats(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Query(query): Query<StatsQuery>,
) -> Result<Json<Value>, ApiError> {
    let row = sqlx::query(
        "SELECT cached_node_count,last_access_at FROM proxy_subscribes WHERE id=$1 AND (user_id=$2 OR authorized_user_ids @> jsonb_build_array($2::text))",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("subscription"))?;
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM proxy_access_logs WHERE subscribe_id=$1")
            .bind(id)
            .fetch_one(&state.pool)
            .await?;
    let today: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM proxy_access_logs WHERE subscribe_id=$1 AND created_at >= CURRENT_DATE")
        .bind(id).fetch_one(&state.pool).await?;
    let by_type = sqlx::query("SELECT access_type,COUNT(*) AS count FROM proxy_access_logs WHERE subscribe_id=$1 GROUP BY access_type ORDER BY count DESC")
        .bind(id).fetch_all(&state.pool).await?;
    let by_type = by_type
        .iter()
        .map(|row| -> Result<Value, ApiError> {
            Ok(json!({
                "type": row.try_get::<String, _>("access_type").map_err(ApiError::internal)?,
                "count": row.try_get::<i64, _>("count").map_err(ApiError::internal)?,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let recent = sqlx::query("SELECT id,subscribe_id,access_type,ip,user_agent,node_count,created_at FROM proxy_access_logs WHERE subscribe_id=$1 ORDER BY created_at DESC LIMIT $2 OFFSET $3")
        .bind(id).bind(page_size).bind((page - 1) * page_size)
        .fetch_all(&state.pool).await?;
    let recent = recent.iter().map(|row| -> Result<Value, ApiError> {
        Ok(json!({
            "id": row.try_get::<Uuid, _>("id").map_err(ApiError::internal)?,
            "subscribeId": row.try_get::<Uuid, _>("subscribe_id").map_err(ApiError::internal)?,
            "accessType": row.try_get::<String, _>("access_type").map_err(ApiError::internal)?,
            "ip": row.try_get::<Option<String>, _>("ip").map_err(ApiError::internal)?,
            "userAgent": row.try_get::<Option<String>, _>("user_agent").map_err(ApiError::internal)?,
            "nodeCount": row.try_get::<Option<i32>, _>("node_count").map_err(ApiError::internal)?,
            "createdAt": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at").map_err(ApiError::internal)?,
        }))
    }).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(json!({
        "totalAccesses": total,
        "todayAccess": today,
        "cachedNodeCount": row.try_get::<Option<i32>, _>("cached_node_count").map_err(ApiError::internal)?.unwrap_or(0),
        "lastAccessAt": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_access_at").map_err(ApiError::internal)?,
        "accessByType": by_type,
        "recentAccessTotal": total,
        "recentAccesses": recent,
    })))
}
