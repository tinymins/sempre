use std::collections::HashSet;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::get,
};
use chrono::{DateTime, Utc};
use sempre_converter::Target;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Row as _, postgres::PgRow};
use uuid::Uuid;

use crate::{
    AppState, auth::CurrentUser, error::ApiError, subscription_selected_nodes::sync_selected,
    subscription_validation::validate,
};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/subscriptions", get(list).post(create))
        .route(
            "/api/v1/subscriptions/{id}",
            get(get_subscription).patch(update).delete(remove),
        )
        .route("/api/v1/subscription-defaults", get(defaults))
        .route("/api/v1/users", get(users))
}

fn default_true() -> bool {
    true
}

fn default_log_level() -> String {
    "info".into()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Toolbox schema has five independent useSystem switches"
)]
pub(crate) struct SubscriptionFields {
    #[serde(default)]
    pub(crate) remark: Option<String>,
    #[serde(default = "default_log_level")]
    pub(crate) log_level: String,
    #[serde(default)]
    pub(crate) subscribe_url: Option<String>,
    #[serde(default)]
    pub(crate) subscribe_items: Option<Value>,
    #[serde(default)]
    pub(crate) rule_list: Option<String>,
    #[serde(default = "default_true")]
    pub(crate) use_system_rule_list: bool,
    #[serde(default)]
    pub(crate) group: Option<String>,
    #[serde(default = "default_true")]
    pub(crate) use_system_group: bool,
    #[serde(default)]
    pub(crate) filter: Option<String>,
    #[serde(default = "default_true")]
    pub(crate) use_system_filter: bool,
    #[serde(default)]
    pub(crate) servers: Option<String>,
    #[serde(default)]
    pub(crate) custom_config: Option<String>,
    #[serde(default = "default_true")]
    pub(crate) use_system_custom_config: bool,
    #[serde(default)]
    pub(crate) dns_config: Option<String>,
    #[serde(default = "default_true")]
    pub(crate) use_system_dns_config: bool,
    #[serde(default)]
    pub(crate) private_access_config: Option<String>,
    #[serde(default)]
    pub(crate) authorized_user_ids: Vec<Uuid>,
    #[serde(default)]
    pub(crate) cache_ttl_minutes: Option<i32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserBrief {
    id: Uuid,
    name: String,
    email: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionOutput {
    id: Uuid,
    user_id: Uuid,
    url: String,
    #[serde(flatten)]
    fields: SubscriptionFields,
    creator: UserBrief,
    assigned_custom_nodes: Vec<Value>,
    selected_custom_node_ids: Vec<Uuid>,
    cached_node_count: i32,
    access_count: i64,
    last_access_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    can_edit: bool,
    can_delete: bool,
    can_manage_authorization: bool,
}

const SELECT_SUBSCRIPTION: &str = "SELECT s.*, u.name AS creator_name, u.email AS creator_email, s.access_total AS access_count FROM proxy_subscribes s JOIN users u ON u.id = s.user_id";

async fn list(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<SubscriptionOutput>>, ApiError> {
    let sql = format!(
        "{SELECT_SUBSCRIPTION} WHERE s.user_id = $1 OR s.authorized_user_ids @> jsonb_build_array($1::text) ORDER BY s.created_at DESC"
    );
    let rows = sqlx::query(&sql)
        .bind(user.id)
        .fetch_all(&state.pool)
        .await?;
    let mut output = Vec::with_capacity(rows.len());
    for row in &rows {
        output.push(row_output(&state.pool, row, user.id).await?);
    }
    Ok(Json(output))
}

async fn get_subscription(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<SubscriptionOutput>, ApiError> {
    let row = visible_row(&state.pool, id, user.id).await?;
    row_output(&state.pool, &row, user.id).await.map(Json)
}

async fn create(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(value): Json<Value>,
) -> Result<(StatusCode, Json<SubscriptionOutput>), ApiError> {
    let (fields, selected) = parse_input(&value)?;
    validate(&state.pool, &fields, &selected, None).await?;
    let id = Uuid::new_v4();
    let url = Uuid::new_v4().to_string();
    let mut transaction = state.pool.begin().await?;
    sqlx::query("INSERT INTO proxy_subscribes (id, user_id, url, remark, log_level, subscribe_url, subscribe_items, rule_list, use_system_rule_list, \"group\", use_system_group, filter, use_system_filter, servers, custom_config, use_system_custom_config, dns_config, use_system_dns_config, private_access_config, authorized_user_ids, cache_ttl_minutes) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)")
        .bind(id).bind(user.id).bind(url)
        .bind(&fields.remark).bind(&fields.log_level).bind(&fields.subscribe_url)
        .bind(&fields.subscribe_items).bind(&fields.rule_list).bind(fields.use_system_rule_list)
        .bind(&fields.group).bind(fields.use_system_group).bind(&fields.filter)
        .bind(fields.use_system_filter).bind(&fields.servers).bind(&fields.custom_config)
        .bind(fields.use_system_custom_config).bind(&fields.dns_config)
        .bind(fields.use_system_dns_config).bind(&fields.private_access_config)
        .bind(json!(fields.authorized_user_ids)).bind(fields.cache_ttl_minutes)
        .execute(&mut *transaction).await?;
    sync_selected(&mut transaction, id, &selected).await?;
    transaction.commit().await?;
    let row = visible_row(&state.pool, id, user.id).await?;
    Ok((
        StatusCode::CREATED,
        Json(row_output(&state.pool, &row, user.id).await?),
    ))
}

async fn update(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(value): Json<Value>,
) -> Result<Json<SubscriptionOutput>, ApiError> {
    let expected = expected_updated_at(&headers)?;
    let mut transaction = state.pool.begin().await?;
    let row = sqlx::query("SELECT * FROM proxy_subscribes WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| ApiError::not_found("subscription"))?;
    let owner: Uuid = row.try_get("user_id").map_err(ApiError::internal)?;
    let authorized: Option<Value> = row
        .try_get("authorized_user_ids")
        .map_err(ApiError::internal)?;
    let authorized = authorized.unwrap_or_else(|| json!([]));
    if owner != user.id
        && !authorized.as_array().is_some_and(|users| {
            users
                .iter()
                .any(|value| value.as_str() == Some(&user.id.to_string()))
        })
    {
        return Err(ApiError::not_found("subscription"));
    }
    let current_updated: DateTime<Utc> = row.try_get("updated_at").map_err(ApiError::internal)?;
    if current_updated != expected {
        return Err(ApiError::conflict(
            "subscription changed; reload before saving",
        ));
    }
    let mut changes = value
        .as_object()
        .cloned()
        .ok_or_else(|| ApiError::bad_request("object body required"))?;
    let confirm_share = changes.remove("confirmShareAssignedNodes") == Some(Value::Bool(true));
    if owner != user.id && changes.contains_key("authorizedUserIds") {
        return Err(ApiError::forbidden(
            "only the owner can manage authorization",
        ));
    }
    let selected = changes.remove("selectedCustomNodeIds");
    let selected: Option<Vec<Uuid>> = selected
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let current = row_fields(&row)?;
    let old_authorized = current.authorized_user_ids.clone();
    let (fields, edited_fields) = merge_patch(current, changes)?;
    validate(
        &state.pool,
        &fields,
        selected.as_deref().unwrap_or(&[]),
        Some(&edited_fields),
    )
    .await?;
    let new_users = fields
        .authorized_user_ids
        .iter()
        .filter(|user_id| !old_authorized.contains(user_id))
        .copied()
        .collect::<Vec<_>>();
    if !new_users.is_empty() {
        let assigned = sqlx::query(
            "SELECT custom_node_id FROM proxy_subscribe_custom_nodes WHERE subscribe_id=$1",
        )
        .bind(id)
        .fetch_all(&mut *transaction)
        .await?;
        if !assigned.is_empty() && !confirm_share {
            return Err(ApiError::conflict(
                "confirm sharing assigned nodes with newly authorized users",
            ));
        }
    }
    sqlx::query("UPDATE proxy_subscribes SET remark=$1,log_level=$2,subscribe_url=$3,subscribe_items=$4,rule_list=$5,use_system_rule_list=$6,\"group\"=$7,use_system_group=$8,filter=$9,use_system_filter=$10,servers=$11,custom_config=$12,use_system_custom_config=$13,dns_config=$14,use_system_dns_config=$15,private_access_config=$16,authorized_user_ids=$17,cache_ttl_minutes=$18,updated_at=GREATEST(clock_timestamp(), updated_at + INTERVAL '1 microsecond') WHERE id=$19")
        .bind(&fields.remark).bind(&fields.log_level).bind(&fields.subscribe_url)
        .bind(&fields.subscribe_items).bind(&fields.rule_list).bind(fields.use_system_rule_list)
        .bind(&fields.group).bind(fields.use_system_group).bind(&fields.filter)
        .bind(fields.use_system_filter).bind(&fields.servers).bind(&fields.custom_config)
        .bind(fields.use_system_custom_config).bind(&fields.dns_config)
        .bind(fields.use_system_dns_config).bind(&fields.private_access_config)
        .bind(json!(fields.authorized_user_ids)).bind(fields.cache_ttl_minutes).bind(id)
        .execute(&mut *transaction).await?;
    if let Some(selected) = selected {
        sync_selected(&mut transaction, id, &selected).await?;
    }
    transaction.commit().await?;
    let row = visible_row(&state.pool, id, user.id).await?;
    row_output(&state.pool, &row, user.id).await.map(Json)
}

fn merge_patch(
    current: SubscriptionFields,
    changes: serde_json::Map<String, Value>,
) -> Result<(SubscriptionFields, HashSet<String>), ApiError> {
    let mut merged = serde_json::to_value(current).map_err(ApiError::internal)?;
    let object = merged
        .as_object_mut()
        .ok_or_else(|| ApiError::internal("subscription fields"))?;
    let mut edited_fields = HashSet::new();
    for (key, value) in changes {
        if !object.contains_key(&key) {
            return Err(ApiError::bad_request(format!(
                "unknown subscription field: {key}"
            )));
        }
        if object.get(&key) != Some(&value) {
            edited_fields.insert(key.clone());
        }
        object.insert(key, value);
    }
    let fields =
        serde_json::from_value(merged).map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok((fields, edited_fields))
}

async fn remove(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let affected = sqlx::query("DELETE FROM proxy_subscribes WHERE id=$1 AND user_id=$2")
        .bind(id)
        .bind(user.id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(ApiError::not_found("subscription"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn users(
    State(state): State<Arc<AppState>>,
    CurrentUser(_user): CurrentUser,
) -> Result<Json<Vec<UserBrief>>, ApiError> {
    let rows = sqlx::query("SELECT id,name,email FROM users ORDER BY name,email")
        .fetch_all(&state.pool)
        .await?;
    rows.iter()
        .map(|row| {
            Ok(UserBrief {
                id: row.try_get("id").map_err(ApiError::internal)?,
                name: row.try_get("name").map_err(ApiError::internal)?,
                email: row.try_get("email").map_err(ApiError::internal)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

#[derive(Deserialize)]
struct DefaultsQuery {
    format: Option<String>,
}

async fn defaults(
    CurrentUser(_user): CurrentUser,
    Query(query): Query<DefaultsQuery>,
) -> Result<Json<Value>, ApiError> {
    let target = query
        .format
        .as_deref()
        .map(Target::parse)
        .transpose()
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(crate::subscription_editor::defaults(target.as_ref())))
}

async fn visible_row(pool: &PgPool, id: Uuid, user_id: Uuid) -> Result<PgRow, ApiError> {
    let sql = format!(
        "{SELECT_SUBSCRIPTION} WHERE s.id=$1 AND (s.user_id=$2 OR s.authorized_user_ids @> jsonb_build_array($2::text))"
    );
    sqlx::query(&sql)
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::not_found("subscription"))
}

pub(crate) fn row_fields(row: &PgRow) -> Result<SubscriptionFields, ApiError> {
    let authorized: Option<Value> = row
        .try_get("authorized_user_ids")
        .map_err(ApiError::internal)?;
    Ok(SubscriptionFields {
        remark: row.try_get("remark").map_err(ApiError::internal)?,
        log_level: row.try_get("log_level").map_err(ApiError::internal)?,
        subscribe_url: row.try_get("subscribe_url").map_err(ApiError::internal)?,
        subscribe_items: row.try_get("subscribe_items").map_err(ApiError::internal)?,
        rule_list: row.try_get("rule_list").map_err(ApiError::internal)?,
        use_system_rule_list: row
            .try_get("use_system_rule_list")
            .map_err(ApiError::internal)?,
        group: row.try_get("group").map_err(ApiError::internal)?,
        use_system_group: row
            .try_get("use_system_group")
            .map_err(ApiError::internal)?,
        filter: row.try_get("filter").map_err(ApiError::internal)?,
        use_system_filter: row
            .try_get("use_system_filter")
            .map_err(ApiError::internal)?,
        servers: row.try_get("servers").map_err(ApiError::internal)?,
        custom_config: row.try_get("custom_config").map_err(ApiError::internal)?,
        use_system_custom_config: row
            .try_get("use_system_custom_config")
            .map_err(ApiError::internal)?,
        dns_config: row.try_get("dns_config").map_err(ApiError::internal)?,
        use_system_dns_config: row
            .try_get("use_system_dns_config")
            .map_err(ApiError::internal)?,
        private_access_config: row
            .try_get("private_access_config")
            .map_err(ApiError::internal)?,
        authorized_user_ids: serde_json::from_value(authorized.unwrap_or_else(|| json!([])))
            .map_err(ApiError::internal)?,
        cache_ttl_minutes: row
            .try_get("cache_ttl_minutes")
            .map_err(ApiError::internal)?,
    })
}

async fn row_output(
    pool: &PgPool,
    row: &PgRow,
    viewer: Uuid,
) -> Result<SubscriptionOutput, ApiError> {
    let id: Uuid = row.try_get("id").map_err(ApiError::internal)?;
    let user_id: Uuid = row.try_get("user_id").map_err(ApiError::internal)?;
    let assignment_rows = sqlx::query("SELECT a.custom_node_id,a.enabled,a.position,n.user_id,n.content FROM proxy_subscribe_custom_nodes a JOIN proxy_custom_nodes n ON n.id=a.custom_node_id WHERE a.subscribe_id=$1 ORDER BY a.position")
        .bind(id).fetch_all(pool).await?;
    let selected_custom_node_ids = assignment_rows
        .iter()
        .filter(|row| row.try_get::<bool, _>("enabled").unwrap_or(false))
        .map(|row| row.try_get("custom_node_id").map_err(ApiError::internal))
        .collect::<Result<Vec<_>, _>>()?;
    let assigned_custom_nodes = assignment_rows
        .iter()
        .map(|row| -> Result<Value, ApiError> {
            let content: String = row.try_get("content").map_err(ApiError::internal)?;
            let proxy = sempre_converter::parse_jsonc_value(&content)
                .map_err(|error| ApiError::bad_request(error.to_string()))?;
            Ok(json!({
                "id": row.try_get::<Uuid, _>("custom_node_id").map_err(ApiError::internal)?,
                "userId": row.try_get::<Uuid, _>("user_id").map_err(ApiError::internal)?,
                "name": proxy.get("name"),
                "proxyType": proxy.get("type"),
                "server": proxy.get("server"),
                "port": proxy.get("port"),
                "enabled": row.try_get::<bool, _>("enabled").map_err(ApiError::internal)?,
                "position": row.try_get::<i32, _>("position").map_err(ApiError::internal)?,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let can_manage = user_id == viewer;
    Ok(SubscriptionOutput {
        id,
        user_id,
        url: row.try_get("url").map_err(ApiError::internal)?,
        fields: row_fields(row)?,
        creator: UserBrief {
            id: user_id,
            name: row.try_get("creator_name").map_err(ApiError::internal)?,
            email: row.try_get("creator_email").map_err(ApiError::internal)?,
        },
        assigned_custom_nodes,
        selected_custom_node_ids,
        cached_node_count: row
            .try_get::<Option<i32>, _>("cached_node_count")
            .map_err(ApiError::internal)?
            .unwrap_or(0),
        access_count: row.try_get("access_count").map_err(ApiError::internal)?,
        last_access_at: row.try_get("last_access_at").map_err(ApiError::internal)?,
        created_at: row
            .try_get::<Option<DateTime<Utc>>, _>("created_at")
            .map_err(ApiError::internal)?
            .unwrap_or_else(Utc::now),
        updated_at: row
            .try_get::<Option<DateTime<Utc>>, _>("updated_at")
            .map_err(ApiError::internal)?
            .unwrap_or_else(Utc::now),
        can_edit: true,
        can_delete: can_manage,
        can_manage_authorization: can_manage,
    })
}

fn expected_updated_at(headers: &HeaderMap) -> Result<DateTime<Utc>, ApiError> {
    let expected = headers
        .get("if-match")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_matches('"'))
        .ok_or_else(|| ApiError::bad_request("If-Match updatedAt is required"))?;
    DateTime::parse_from_rfc3339(expected)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| ApiError::bad_request("If-Match must be an RFC3339 timestamp"))
}

pub(crate) fn parse_input(value: &Value) -> Result<(SubscriptionFields, Vec<Uuid>), ApiError> {
    let mut object = value
        .as_object()
        .cloned()
        .ok_or_else(|| ApiError::bad_request("object body required"))?;
    let selected = object
        .remove("selectedCustomNodeIds")
        .unwrap_or_else(|| json!([]));
    let selected: Vec<Uuid> = serde_json::from_value(selected)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let fields = serde_json::from_value(Value::Object(object))
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok((fields, selected))
}
