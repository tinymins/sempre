use std::sync::Arc;

use axum::{Json, Router, extract::State, routing::get};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::Row as _;
use uuid::Uuid;

use crate::{AppState, auth::CurrentUser, error::ApiError};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Creator {
    id: Uuid,
    name: String,
    email: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionSummary {
    id: Uuid,
    remark: Option<String>,
    creator: Creator,
    last_access_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Overview {
    total_subscriptions: i64,
    total_nodes: i64,
    today_requests: i64,
    top_subscriptions: Vec<SubscriptionSummary>,
}

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/overview", get(overview))
}

async fn overview(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Overview>, ApiError> {
    let visible = "s.user_id=$1 OR s.authorized_user_ids @> jsonb_build_array($1::text)";
    let totals = sqlx::query(&format!(
        "SELECT COUNT(*) AS total_subscriptions, COALESCE(SUM(s.cached_node_count),0) AS total_nodes FROM proxy_subscribes s WHERE {visible}"
    )).bind(user.id).fetch_one(&state.pool).await?;
    let today_requests: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM proxy_access_logs l JOIN proxy_subscribes s ON s.id=l.subscribe_id WHERE ({visible}) AND l.created_at >= date_trunc('day', NOW() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'"
    )).bind(user.id).fetch_one(&state.pool).await?;
    let rows = sqlx::query(&format!(
        "SELECT s.id,s.remark,s.last_access_at,u.id AS creator_id,u.name AS creator_name,u.email AS creator_email FROM proxy_subscribes s JOIN users u ON u.id=s.user_id WHERE {visible} ORDER BY s.created_at DESC LIMIT 5"
    )).bind(user.id).fetch_all(&state.pool).await?;
    let top_subscriptions = rows
        .iter()
        .map(|row| -> Result<SubscriptionSummary, ApiError> {
            Ok(SubscriptionSummary {
                id: row.try_get("id").map_err(ApiError::internal)?,
                remark: row.try_get("remark").map_err(ApiError::internal)?,
                last_access_at: row.try_get("last_access_at").map_err(ApiError::internal)?,
                creator: Creator {
                    id: row.try_get("creator_id").map_err(ApiError::internal)?,
                    name: row.try_get("creator_name").map_err(ApiError::internal)?,
                    email: row.try_get("creator_email").map_err(ApiError::internal)?,
                },
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(Overview {
        total_subscriptions: totals
            .try_get("total_subscriptions")
            .map_err(ApiError::internal)?,
        total_nodes: totals.try_get("total_nodes").map_err(ApiError::internal)?,
        today_requests,
        top_subscriptions,
    }))
}
