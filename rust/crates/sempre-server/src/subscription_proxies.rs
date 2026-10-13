use crate::{
    AppState,
    auth::CurrentUser,
    error::ApiError,
    proxy_crypto, proxy_node_binding,
    proxy_runtime::{self, Stored},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use sempre_proxy::Engine;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Row as _, postgres::PgRow};
use std::sync::Arc;
use uuid::Uuid;

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/subscription-proxies", get(list).post(create))
        .route(
            "/api/v1/subscription-proxies/{id}",
            get(edit).patch(update).delete(remove),
        )
        .route(
            "/api/v1/subscription-proxies/{id}/test",
            axum::routing::post(test),
        )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    name: String,
    content: Option<String>,
    custom_node_id: Option<Uuid>,
    engine: Option<Engine>,
    enabled: bool,
}

async fn list(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query("SELECT p.*, (SELECT COUNT(*) FROM proxy_subscribes s WHERE EXISTS (SELECT 1 FROM jsonb_array_elements(COALESCE(s.subscribe_items,'[]'::jsonb)) item WHERE item->>'proxyId'=p.id::text)) + (SELECT COUNT(*) FROM relay_egress_proxy_references r WHERE r.proxy_id=p.id) AS reference_count FROM subscription_proxies p ORDER BY p.created_at")
        .fetch_all(&state.pool).await?;
    Ok(Json(
        json!({"configured":state.config.proxy.is_some(),"items":rows.iter().map(|row| output(row, user.id)).collect::<Result<Vec<_>, _>>()?}),
    ))
}

fn output(row: &PgRow, viewer: Uuid) -> Result<Value, ApiError> {
    let owner: Uuid = row.try_get("user_id")?;
    Ok(json!({
        "id":row.try_get::<Uuid,_>("id")?,"name":row.try_get::<String,_>("name")?,
        "customNodeId":row.try_get::<Option<Uuid>,_>("custom_node_id")?,
        "engine":row.try_get::<String,_>("engine")?,"protocol":row.try_get::<String,_>("protocol")?,
        "enabled":row.try_get::<bool,_>("enabled")?,"status":row.try_get::<String,_>("status")?,
        "revision":row.try_get::<i64,_>("revision")?,"activeRevision":row.try_get::<Option<i64>,_>("active_revision")?,
        "lastError":row.try_get::<Option<String>,_>("last_error")?,
        "checkedAt":row.try_get::<Option<chrono::DateTime<chrono::Utc>>,_>("checked_at")?,
        "canEdit":owner==viewer,
        "referenceCount":row.try_get::<i64,_>("reference_count").unwrap_or(0)
    }))
}

async fn edit(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let row = proxy_runtime::visible_row(&state, id).await?;
    require_owner(&row, user.id)?;
    let stored = proxy_runtime::decode(&state, id, &row.try_get::<String, _>("configuration")?)?;
    let mut output = output(&row, user.id)?;
    output["content"] = json!(stored.content);
    Ok(Json(output))
}

fn require_owner(row: &PgRow, user: Uuid) -> Result<(), ApiError> {
    if row.try_get::<Uuid, _>("user_id")? != user {
        return Err(ApiError::forbidden(
            "only the owner can manage subscription proxies",
        ));
    }
    Ok(())
}

fn validate(input: &Input) -> Result<(), ApiError> {
    if input.custom_node_id.is_some() && input.content.is_some() {
        return Err(ApiError::bad_request(
            "choose a custom node or provide proxy configuration, not both",
        ));
    }
    if input.name.trim().is_empty() || input.name.chars().count() > 80 {
        return Err(ApiError::bad_request(
            "proxy name must contain 1–80 characters",
        ));
    }
    Ok(())
}

pub(crate) fn encode(
    state: &AppState,
    id: Uuid,
    content: &str,
    engine: Option<Engine>,
) -> Result<(String, String, String), ApiError> {
    let connection = sempre_proxy::parse(content, engine).map_err(ApiError::bad_request)?;
    let engine = match connection.engine {
        Engine::SingBox => "sing-box",
        Engine::Xray => "xray",
    }
    .to_owned();
    let protocol = connection.protocol.clone();
    let stored = serde_json::to_vec(&Stored {
        content: content.to_owned(),
        connection,
    })
    .map_err(ApiError::internal)?;
    Ok((
        proxy_crypto::encrypt(&proxy_runtime::config(state)?.key, id, &stored)?,
        engine,
        protocol,
    ))
}

async fn create(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<Input>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    validate(&input)?;
    let id = Uuid::new_v4();
    let mut tx = state.pool.begin().await?;
    let content = match input.custom_node_id {
        Some(node) => proxy_node_binding::content(&mut tx, node, user.id).await?,
        None => input
            .content
            .clone()
            .ok_or_else(|| ApiError::bad_request("proxy configuration required"))?,
    };
    let (cipher, engine, protocol) = encode(&state, id, &content, input.engine)?;
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM subscription_proxies WHERE user_id=$1")
            .bind(user.id)
            .fetch_one(&mut *tx)
            .await?;
    if count >= 8 {
        return Err(ApiError::bad_request(
            "maximum 8 subscription proxies per user",
        ));
    }
    let row = sqlx::query("INSERT INTO subscription_proxies (id,user_id,name,engine,protocol,configuration,enabled,status,custom_node_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *")
        .bind(id).bind(user.id).bind(input.name.trim()).bind(engine).bind(protocol).bind(cipher).bind(input.enabled)
        .bind(if input.enabled { "starting" } else { "stopped" }).bind(input.custom_node_id).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    schedule(state, id);
    Ok((StatusCode::CREATED, Json(output(&row, user.id)?)))
}

async fn update(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<Input>,
) -> Result<Json<Value>, ApiError> {
    validate(&input)?;
    let mut tx = state.pool.begin().await?;
    let content = match input.custom_node_id {
        Some(node) => Some(proxy_node_binding::content(&mut tx, node, user.id).await?),
        None => input.content.clone(),
    };
    let row = sqlx::query("SELECT * FROM subscription_proxies WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::not_found("subscription proxy"))?;
    require_owner(&row, user.id)?;
    let encoded = content
        .as_ref()
        .map(|content| encode(&state, id, content, input.engine))
        .transpose()?;
    let (cipher, engine, protocol) = match encoded {
        Some(value) => value,
        None => (
            row.try_get("configuration")?,
            row.try_get("engine")?,
            row.try_get("protocol")?,
        ),
    };
    let revision: i64 = row.try_get("revision")?;
    let next_revision = if content.is_some() {
        revision + 1
    } else {
        revision
    };
    let custom_node_id = if content.is_some() {
        input.custom_node_id
    } else {
        row.try_get("custom_node_id")?
    };
    let row = sqlx::query("UPDATE subscription_proxies SET name=$2,configuration=$3,engine=$4,protocol=$5,enabled=$6,revision=$7,status=$8,last_error=NULL,updated_at=NOW(),custom_node_id=$9 WHERE id=$1 RETURNING *")
        .bind(id).bind(input.name.trim()).bind(cipher).bind(engine).bind(protocol).bind(input.enabled)
        .bind(next_revision).bind(if input.enabled { "starting" } else { "stopping" }).bind(custom_node_id).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    schedule(state, id);
    Ok(Json(output(&row, user.id)?))
}

async fn remove(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let mut tx = state.pool.begin().await?;
    let row = sqlx::query("SELECT * FROM subscription_proxies WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::not_found("subscription proxy"))?;
    require_owner(&row, user.id)?;
    let referenced: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM relay_egress_proxy_references WHERE proxy_id::text=$1) OR EXISTS (SELECT 1 FROM proxy_subscribes s WHERE EXISTS (SELECT 1 FROM jsonb_array_elements(COALESCE(s.subscribe_items,'[]'::jsonb)) item WHERE item->>'proxyId'=$1::text))").bind(id.to_string()).fetch_one(&mut *tx).await?;
    if referenced {
        return Err(ApiError::conflict(
            "proxy is referenced by subscriptions or relay egress policy; choose a replacement first",
        ));
    }
    proxy_runtime::stop(&state, id).await?;
    sqlx::query("DELETE FROM subscription_proxies WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn test(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let row = proxy_runtime::visible_row(&state, id).await?;
    if !row.try_get::<bool, _>("enabled")? {
        return Err(ApiError::bad_request("enable the proxy before testing"));
    }
    proxy_runtime::reconcile(&state, id).await?;
    let route = proxy_runtime::resolve(&state, user.id, Some(id)).await?;
    let started = std::time::Instant::now();
    let result = crate::fetch::fetch_source_text(
        "https://www.baidu.com/",
        "Sempre",
        route.url.as_deref(),
        |_| {},
    )
    .await;
    match result {
        Ok(_) => Ok(Json(
            json!({"ok":true,"elapsedMs":started.elapsed().as_millis(),"name":route.name}),
        )),
        Err(error) => Err(error),
    }
}

pub(crate) fn schedule(state: Arc<AppState>, id: Uuid) {
    tokio::spawn(async move {
        if let Err(error) = proxy_runtime::reconcile(&state, id).await {
            tracing::warn!(
                message = error.message(),
                "subscription proxy update failed"
            );
        }
    });
}

pub(crate) async fn validate_sources(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    fields: &crate::subscriptions::SubscriptionFields,
) -> Result<(), ApiError> {
    for item in crate::subscription_sources::source_items(fields)? {
        if let Some(id) = item.proxy_id {
            // Reference locks prevent deletion while a source is being saved.
            sqlx::query("SELECT id FROM subscription_proxies WHERE id=$1 FOR KEY SHARE")
                .bind(id)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or_else(|| ApiError::not_found("subscription proxy"))?;
        }
    }
    Ok(())
}
