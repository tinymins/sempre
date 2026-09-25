use std::{collections::HashSet, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row as _, Transaction, postgres::PgRow};
use uuid::Uuid;

use sempre_converter::{Proxy, parse_jsonc_value};

use crate::{AppState, auth::CurrentUser, error::ApiError};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/custom-nodes", get(list).post(create))
        .route(
            "/api/v1/custom-nodes/{id}",
            get(get_node).patch(update).delete(remove),
        )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NodeInput {
    content: String,
    #[serde(default)]
    authorized_user_ids: Vec<Uuid>,
    #[serde(default)]
    assigned_subscribe_ids: Vec<Uuid>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NodePatch {
    content: Option<String>,
    authorized_user_ids: Option<Vec<Uuid>>,
    assigned_subscribe_ids: Option<Vec<Uuid>>,
    confirm_unassign_enabled: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserBrief {
    id: Uuid,
    name: String,
    email: String,
}

async fn list(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<Value>>, ApiError> {
    let rows = sqlx::query("SELECT n.*,u.name AS creator_name,u.email AS creator_email FROM proxy_custom_nodes n JOIN users u ON u.id=n.user_id WHERE n.user_id=$1 OR n.authorized_user_ids @> jsonb_build_array($1::text) ORDER BY n.created_at DESC")
        .bind(user.id).fetch_all(&state.pool).await?;
    let mut output = Vec::with_capacity(rows.len());
    for row in &rows {
        output.push(node_output(&state, row, user.id).await?);
    }
    Ok(Json(output))
}

async fn get_node(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let row = visible_row(&state, id, user.id).await?;
    node_output(&state, &row, user.id).await.map(Json)
}

async fn create(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<NodeInput>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    parse_content(&input.content)?;
    validate_users(&state, &input.authorized_user_ids).await?;
    let id = Uuid::new_v4();
    let mut tx = state.pool.begin().await?;
    sqlx::query("INSERT INTO proxy_custom_nodes (id,user_id,content,authorized_user_ids) VALUES ($1,$2,$3,$4)")
        .bind(id).bind(user.id).bind(&input.content).bind(json!(input.authorized_user_ids))
        .execute(&mut *tx).await?;
    sync_assignments(&mut tx, id, user.id, &input.assigned_subscribe_ids, false).await?;
    tx.commit().await?;
    let row = visible_row(&state, id, user.id).await?;
    Ok((
        StatusCode::CREATED,
        Json(node_output(&state, &row, user.id).await?),
    ))
}

async fn update(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<NodePatch>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = state.pool.begin().await?;
    let row = sqlx::query("SELECT * FROM proxy_custom_nodes WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::not_found("custom node"))?;
    let owner: Uuid = row.try_get("user_id").map_err(ApiError::internal)?;
    let authorized: Option<Value> = row
        .try_get("authorized_user_ids")
        .map_err(ApiError::internal)?;
    let editable = owner == user.id
        || authorized
            .as_ref()
            .and_then(Value::as_array)
            .is_some_and(|ids| {
                ids.iter()
                    .any(|value| value.as_str() == Some(&user.id.to_string()))
            });
    if !editable {
        return Err(ApiError::not_found("custom node"));
    }
    if owner != user.id
        && (input.authorized_user_ids.is_some() || input.assigned_subscribe_ids.is_some())
    {
        return Err(ApiError::forbidden(
            "only the owner can share or assign this node",
        ));
    }
    if let Some(content) = &input.content {
        parse_content(content)?;
    }
    if let Some(ids) = &input.authorized_user_ids {
        validate_users(&state, ids).await?;
    }
    sqlx::query("UPDATE proxy_custom_nodes SET content=COALESCE($1,content),authorized_user_ids=COALESCE($2,authorized_user_ids),updated_at=NOW() WHERE id=$3")
        .bind(&input.content)
        .bind(input.authorized_user_ids.map(|ids| json!(ids)))
        .bind(id).execute(&mut *tx).await?;
    if let Some(ids) = input.assigned_subscribe_ids {
        sync_assignments(
            &mut tx,
            id,
            user.id,
            &ids,
            input.confirm_unassign_enabled.unwrap_or(false),
        )
        .await?;
    }
    tx.commit().await?;
    let row = visible_row(&state, id, user.id).await?;
    node_output(&state, &row, user.id).await.map(Json)
}

async fn remove(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let affected = sqlx::query("DELETE FROM proxy_custom_nodes WHERE id=$1 AND user_id=$2")
        .bind(id)
        .bind(user.id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(ApiError::not_found("custom node"));
    }
    Ok(StatusCode::NO_CONTENT)
}

fn parse_content(content: &str) -> Result<Proxy, ApiError> {
    let value =
        parse_jsonc_value(content).map_err(|error| ApiError::bad_request(error.to_string()))?;
    let proxy =
        Proxy::from_value(value).map_err(|error| ApiError::bad_request(error.to_string()))?;
    if proxy.name.trim().is_empty()
        || proxy.proxy_type.trim().is_empty()
        || proxy.server.trim().is_empty()
        || proxy.port == 0
    {
        return Err(ApiError::bad_request(
            "node requires non-empty name, type, server and a valid port",
        ));
    }
    Ok(proxy)
}

async fn validate_users(state: &AppState, ids: &[Uuid]) -> Result<(), ApiError> {
    let unique = ids.iter().copied().collect::<HashSet<_>>();
    if unique.len() != ids.len() {
        return Err(ApiError::bad_request(
            "authorizedUserIds contains duplicates",
        ));
    }
    if ids.is_empty() {
        return Ok(());
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id=ANY($1)")
        .bind(ids)
        .fetch_one(&state.pool)
        .await?;
    if usize::try_from(count).ok() != Some(ids.len()) {
        return Err(ApiError::bad_request("authorized user does not exist"));
    }
    Ok(())
}

async fn sync_assignments(
    tx: &mut Transaction<'_, Postgres>,
    node_id: Uuid,
    actor: Uuid,
    desired: &[Uuid],
    confirm_unassign_enabled: bool,
) -> Result<(), ApiError> {
    let unique = desired.iter().copied().collect::<HashSet<_>>();
    if unique.len() != desired.len() {
        return Err(ApiError::bad_request(
            "assignedSubscribeIds contains duplicates",
        ));
    }
    let old = sqlx::query("SELECT subscribe_id,enabled FROM proxy_subscribe_custom_nodes WHERE custom_node_id=$1 FOR UPDATE")
        .bind(node_id).fetch_all(&mut **tx).await?;
    let existing = old
        .iter()
        .map(|row| {
            row.try_get::<Uuid, _>("subscribe_id")
                .map_err(ApiError::internal)
        })
        .collect::<Result<HashSet<_>, _>>()?;
    for row in &old {
        let subscribe_id: Uuid = row.try_get("subscribe_id").map_err(ApiError::internal)?;
        let enabled: bool = row.try_get("enabled").map_err(ApiError::internal)?;
        if !unique.contains(&subscribe_id) && enabled && !confirm_unassign_enabled {
            return Err(ApiError::conflict(
                "removing this assignment disables an active node; confirmation required",
            ));
        }
    }
    for (position, subscribe_id) in desired.iter().enumerate() {
        if !existing.contains(subscribe_id) {
            sqlx::query("SELECT 1 FROM proxy_subscribes WHERE id=$1 AND (user_id=$2 OR authorized_user_ids @> jsonb_build_array($2::text))")
                .bind(subscribe_id).bind(actor).fetch_optional(&mut **tx).await?
                .ok_or_else(|| ApiError::forbidden("node can only be assigned to an editable subscription"))?;
        }
        sqlx::query("INSERT INTO proxy_subscribe_custom_nodes (subscribe_id,custom_node_id,enabled,position) VALUES ($1,$2,FALSE,$3) ON CONFLICT (subscribe_id,custom_node_id) DO NOTHING")
            .bind(subscribe_id).bind(node_id).bind(i32::try_from(position).map_err(ApiError::internal)?)
            .execute(&mut **tx).await?;
    }
    sqlx::query("DELETE FROM proxy_subscribe_custom_nodes WHERE custom_node_id=$1 AND NOT (subscribe_id=ANY($2))")
        .bind(node_id).bind(desired).execute(&mut **tx).await?;
    Ok(())
}

async fn visible_row(state: &AppState, id: Uuid, user_id: Uuid) -> Result<PgRow, ApiError> {
    sqlx::query("SELECT n.*,u.name AS creator_name,u.email AS creator_email FROM proxy_custom_nodes n JOIN users u ON u.id=n.user_id WHERE n.id=$1 AND (n.user_id=$2 OR n.authorized_user_ids @> jsonb_build_array($2::text))")
        .bind(id).bind(user_id).fetch_optional(&state.pool).await?
        .ok_or_else(|| ApiError::not_found("custom node"))
}

async fn node_output(state: &AppState, row: &PgRow, viewer: Uuid) -> Result<Value, ApiError> {
    let id: Uuid = row.try_get("id").map_err(ApiError::internal)?;
    let user_id: Uuid = row.try_get("user_id").map_err(ApiError::internal)?;
    let content: String = row.try_get("content").map_err(ApiError::internal)?;
    let proxy = parse_content(&content)?;
    let owner = user_id == viewer;
    let assignments = if owner {
        sqlx::query("SELECT a.subscribe_id,a.enabled,a.position,s.remark FROM proxy_subscribe_custom_nodes a JOIN proxy_subscribes s ON s.id=a.subscribe_id WHERE a.custom_node_id=$1 ORDER BY a.position")
            .bind(id).fetch_all(&state.pool).await?
    } else {
        Vec::new()
    };
    let assignments = assignments
        .iter()
        .map(|row| -> Result<Value, ApiError> {
            Ok(json!({
                "subscribeId": row.try_get::<Uuid, _>("subscribe_id").map_err(ApiError::internal)?,
                "remark": row.try_get::<Option<String>, _>("remark").map_err(ApiError::internal)?,
                "enabled": row.try_get::<bool, _>("enabled").map_err(ApiError::internal)?,
                "position": row.try_get::<i32, _>("position").map_err(ApiError::internal)?,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "id": id,
        "userId": user_id,
        "content": content,
        "name": proxy.name,
        "proxyType": proxy.proxy_type,
        "server": proxy.server,
        "port": proxy.port,
        "authorizedUserIds": row.try_get::<Option<Value>, _>("authorized_user_ids").map_err(ApiError::internal)?.unwrap_or_else(|| json!([])),
        "creator": UserBrief {
            id: user_id,
            name: row.try_get("creator_name").map_err(ApiError::internal)?,
            email: row.try_get("creator_email").map_err(ApiError::internal)?,
        },
        "assignments": assignments,
        "createdAt": row.try_get::<Option<DateTime<Utc>>, _>("created_at").map_err(ApiError::internal)?,
        "updatedAt": row.try_get::<Option<DateTime<Utc>>, _>("updated_at").map_err(ApiError::internal)?,
        "canEdit": true,
        "canManageAuthorization": owner,
    }))
}
