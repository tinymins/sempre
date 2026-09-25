use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row as _;
use uuid::Uuid;

use sempre_converter::{
    CompileRequest, CompileResult, CustomNode, Diagnostic, EditorConfig, Profile, Target, compile,
    parse_jsonc_value, preview_nodes, trace_node_steps,
};

use crate::{
    AppState,
    auth::CurrentUser,
    error::ApiError,
    source_cache::CacheMode,
    subscription_rules::load_rule_snapshots,
    subscription_sources::load_sources,
    subscriptions::{SubscriptionFields, parse_input, row_fields},
};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/subscriptions/debug", post(debug))
        .route("/api/v1/subscriptions/{id}/preview-nodes", post(preview))
        .route("/api/v1/subscriptions/{id}/trace-node", post(trace))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DebugInput {
    draft: Value,
    target: Target,
    subscription_id: Option<Uuid>,
}

#[derive(Deserialize)]
struct TargetInput {
    target: Target,
}

#[derive(Deserialize)]
struct TraceInput {
    target: Target,
    name: String,
}

pub(crate) struct PrepareOptions {
    pub viewer: Uuid,
    pub cache_mode: CacheMode,
    pub node_scope: Option<Uuid>,
    pub include_rule_snapshots: bool,
}

async fn debug(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<DebugInput>,
) -> Result<Json<Value>, ApiError> {
    let mut stages = Vec::new();
    let cache_mode = if let Some(id) = input.subscription_id {
        saved_input(&state, id, user.id).await?;
        CacheMode::ReadOnlySubscription(id)
    } else {
        CacheMode::ReadOnlyGlobal
    };
    let (fields, selected) = match parse_input(&input.draft) {
        Ok(parsed) => {
            stages.push(json!({"type":"draft","status":"ok"}));
            parsed
        }
        Err(error) => return Ok(Json(debug_failure(stages, "draft", error.message()))),
    };
    let (request, diagnostics) = match prepare(
        &state,
        &fields,
        &selected,
        input.target,
        PrepareOptions {
            viewer: user.id,
            cache_mode,
            node_scope: input.subscription_id,
            include_rule_snapshots: true,
        },
        &mut stages,
    )
    .await
    {
        Ok(prepared) => prepared,
        Err(error) => return Ok(Json(debug_failure(stages, "prepare", error.message()))),
    };
    let mut result = match compile(&request) {
        Ok(result) => result,
        Err(error) => return Ok(Json(debug_failure(stages, "compile", &error.to_string()))),
    };
    result.diagnostics.splice(0..0, diagnostics);
    if result.node_count == 0 {
        result.diagnostics.push(Diagnostic {
            level: "warning".into(),
            source_id: None,
            message: "no proxy nodes in this output; direct-only configurations are valid, but check enabled sources, manual nodes, assigned nodes, and filters if nodes were expected".into(),
        });
        stages.push(json!({"type":"compile","status":"ok","nodeCount":0,"message":"compiled with no proxy nodes"}));
    } else {
        stages.push(json!({"type":"compile","status":"ok","nodeCount":result.node_count}));
    }
    Ok(Json(result_output(&result, &stages)))
}

async fn preview(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<TargetInput>,
) -> Result<Json<Value>, ApiError> {
    let (fields, selected) = saved_input(&state, id, user.id).await?;
    let (request, _) = prepare(
        &state,
        &fields,
        &selected,
        input.target,
        PrepareOptions {
            viewer: user.id,
            cache_mode: CacheMode::Subscription(id),
            node_scope: Some(id),
            include_rule_snapshots: false,
        },
        &mut Vec::new(),
    )
    .await?;
    let nodes =
        preview_nodes(&request).map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(json!({ "nodes": nodes })))
}

async fn trace(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<TraceInput>,
) -> Result<Json<Value>, ApiError> {
    let (fields, selected) = saved_input(&state, id, user.id).await?;
    let (request, _) = prepare(
        &state,
        &fields,
        &selected,
        input.target,
        PrepareOptions {
            viewer: user.id,
            cache_mode: CacheMode::Subscription(id),
            node_scope: Some(id),
            include_rule_snapshots: false,
        },
        &mut Vec::new(),
    )
    .await?;
    let trace = trace_node_steps(&request, &input.name)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(trace))
}

pub(crate) async fn saved_input(
    state: &AppState,
    id: Uuid,
    viewer: Uuid,
) -> Result<(SubscriptionFields, Vec<Uuid>), ApiError> {
    let row = sqlx::query("SELECT * FROM proxy_subscribes WHERE id=$1 AND (user_id=$2 OR authorized_user_ids @> jsonb_build_array($2::text))")
        .bind(id).bind(viewer).fetch_optional(&state.pool).await?
        .ok_or_else(|| ApiError::not_found("subscription"))?;
    let selected_rows = sqlx::query("SELECT custom_node_id FROM proxy_subscribe_custom_nodes WHERE subscribe_id=$1 AND enabled=TRUE ORDER BY position")
        .bind(id).fetch_all(&state.pool).await?;
    let selected = selected_rows
        .iter()
        .map(|row| row.try_get("custom_node_id").map_err(ApiError::internal))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((row_fields(&row)?, selected))
}

pub(crate) async fn prepare(
    state: &AppState,
    fields: &SubscriptionFields,
    selected: &[Uuid],
    target: Target,
    options: PrepareOptions,
    stages: &mut Vec<Value>,
) -> Result<(CompileRequest, Vec<Diagnostic>), ApiError> {
    let mut target =
        Target::parse(&target.format).map_err(|error| ApiError::bad_request(error.to_string()))?;
    target.standalone = true;
    let mut profile = Profile {
        name: fields
            .remark
            .clone()
            .unwrap_or_else(|| "Subscription".into()),
        log_level: fields.log_level.clone(),
        editor: EditorConfig {
            rule_list: effective(
                fields.use_system_rule_list,
                fields.rule_list.as_ref(),
                include_str!("toolbox_defaults/rules.jsonc"),
            ),
            group: effective(
                fields.use_system_group,
                fields.group.as_ref(),
                include_str!("toolbox_defaults/groups.jsonc"),
            ),
            filter: effective(
                fields.use_system_filter,
                fields.filter.as_ref(),
                "[\"官网\",\"客服\",\"qq群\"]",
            ),
            custom_config: effective(
                fields.use_system_custom_config,
                fields.custom_config.as_ref(),
                "[]",
            ),
            dns_config: effective(
                fields.use_system_dns_config,
                fields.dns_config.as_ref(),
                include_str!("toolbox_defaults/dns.jsonc"),
            ),
            private_access_config: fields.private_access_config.clone().unwrap_or_default(),
            servers: fields.servers.clone().unwrap_or_default(),
        },
        ..Profile::default()
    };
    let mut snapshots = Vec::new();
    let mut diagnostics = Vec::new();
    load_sources(
        state,
        fields,
        &mut profile,
        options.cache_mode,
        stages,
        &mut snapshots,
        &mut diagnostics,
    )
    .await?;
    if options.include_rule_snapshots && target.core == "sing-box" {
        load_rule_snapshots(
            state,
            &profile,
            options.cache_mode,
            fields.cache_ttl_minutes.unwrap_or(60),
            &mut snapshots,
            &mut diagnostics,
            stages,
        )
        .await?;
    }
    let custom_nodes = load_custom_nodes(state, selected, options.viewer, options.node_scope)
        .await
        .inspect_err(|error| {
            stages.push(json!({"type":"custom-nodes","status":"error","message":error.message()}));
        })?;
    stages.push(json!({"type":"custom-nodes","status":if selected.is_empty() { "skipped" } else { "ok" },"count":custom_nodes.len()}));
    profile.custom_node_ids = custom_nodes.iter().map(|node| node.id.clone()).collect();
    Ok((
        CompileRequest {
            protocol: 1,
            profile,
            snapshots,
            custom_nodes,
            target,
        },
        diagnostics,
    ))
}

fn effective(use_system: bool, custom: Option<&String>, default: &str) -> String {
    if use_system {
        default.into()
    } else {
        custom.cloned().unwrap_or_default()
    }
}

async fn load_custom_nodes(
    state: &AppState,
    selected: &[Uuid],
    viewer: Uuid,
    node_scope: Option<Uuid>,
) -> Result<Vec<CustomNode>, ApiError> {
    let mut output = Vec::with_capacity(selected.len());
    for id in selected {
        let row = if let Some(subscribe_id) = node_scope {
            sqlx::query("SELECT n.content FROM proxy_custom_nodes n JOIN proxy_subscribe_custom_nodes a ON a.custom_node_id=n.id AND a.subscribe_id=$3 JOIN proxy_subscribes s ON s.id=a.subscribe_id WHERE n.id=$1 AND (s.user_id=$2 OR s.authorized_user_ids @> jsonb_build_array($2::text))")
                .bind(id).bind(viewer).bind(subscribe_id).fetch_optional(&state.pool).await?
        } else {
            sqlx::query("SELECT content FROM proxy_custom_nodes WHERE id=$1 AND (user_id=$2 OR authorized_user_ids @> jsonb_build_array($2::text))")
                .bind(id).bind(viewer).fetch_optional(&state.pool).await?
        }
            .ok_or_else(|| ApiError::forbidden("custom node is not available"))?;
        let content: String = row.try_get("content").map_err(ApiError::internal)?;
        let proxy = parse_jsonc_value(&content)
            .map_err(|error| ApiError::bad_request(error.to_string()))?;
        output.push(CustomNode {
            id: id.to_string(),
            name: proxy
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            proxy,
            created_at: None,
            updated_at: None,
        });
    }
    Ok(output)
}

fn debug_failure(mut stages: Vec<Value>, stage: &str, message: &str) -> Value {
    if !stages
        .last()
        .is_some_and(|value| value.get("status") == Some(&json!("error")))
    {
        stages.push(json!({"type":stage,"status":"error","message":message}));
    }
    json!({"ok":false,"diagnostics":[{"level":"error","message":message}],"stages":stages})
}

fn result_output(result: &CompileResult, stages: &[Value]) -> Value {
    json!({
        "ok": true,
        "format": result.format,
        "content": result.content,
        "nodeCount": result.node_count,
        "diagnostics": result.diagnostics,
        "fieldDiffs": result.field_diffs,
        "stages": stages
    })
}
