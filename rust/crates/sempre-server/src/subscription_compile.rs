use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    response::Response,
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
    debug_stream::{self, StageLog},
    diagnostic_projection,
    error::ApiError,
    source_cache::CacheMode,
    subscription_rules::load_rule_snapshots,
    subscription_sources::{all_sources_failed, load_sources},
    subscription_validation::effective_default_on_empty,
    subscriptions::{SubscriptionFields, parse_input, row_fields},
};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/subscriptions/debug", post(debug))
        .route("/api/v1/subscriptions/{id}/debug", post(saved_debug))
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

pub(crate) struct PreparedInput {
    pub profile: Profile,
    pub custom_nodes: Vec<CustomNode>,
    pub target: Target,
}

async fn debug(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<DebugInput>,
) -> Result<Response, ApiError> {
    let cache_mode = if let Some(id) = input.subscription_id {
        saved_input(&state, id, user.id).await?;
        CacheMode::ReadOnlySubscription(id)
    } else {
        CacheMode::ReadOnlyGlobal
    };
    let (fields, selected) = parse_input(&input.draft)?;
    Ok(debug_stream::response(move |mut stages| async move {
        stages.push(json!({"type":"draft","status":"ok"}));
        run_debug(
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
    }))
}

async fn saved_debug(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<TargetInput>,
) -> Result<Response, ApiError> {
    let (fields, selected) = saved_input(&state, id, user.id).await?;
    Ok(debug_stream::response(move |mut stages| async move {
        stages.push(json!({"type":"saved-subscription","status":"ok"}));
        run_debug(
            &state,
            &fields,
            &selected,
            input.target,
            PrepareOptions {
                viewer: user.id,
                cache_mode: CacheMode::ReadOnlySubscription(id),
                node_scope: Some(id),
                include_rule_snapshots: true,
            },
            &mut stages,
        )
        .await
    }))
}

async fn run_debug(
    state: &AppState,
    fields: &SubscriptionFields,
    selected: &[Uuid],
    target: Target,
    options: PrepareOptions,
    stages: &mut StageLog,
) -> Value {
    stages.push(json!({"type":"prepare","status":"running"}));
    let (request, diagnostics) =
        match prepare(state, fields, selected, target, options, stages).await {
            Ok(prepared) => prepared,
            Err(error) => return debug_failure(stages.clone(), "prepare", error.message()),
        };
    stages.push(json!({"type":"prepare","status":"ok"}));
    stages.push(json!({"type":"compile","status":"running"}));
    let mut result = match compile(&request) {
        Ok(result) => result,
        Err(error) => return debug_failure(stages.clone(), "compile", &error.to_string()),
    };
    if result.node_count == 0 && all_sources_failed(stages) {
        return debug_failure(
            stages.clone(),
            "compile",
            "all enabled subscription sources failed and no usable nodes remain",
        );
    }
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
    let (node_traces, rule_samples) = diagnostic_projection::project(&request, &result, stages);
    result_output(&result, stages, &node_traces, &rule_samples)
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
        &mut StageLog::default(),
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
        &mut StageLog::default(),
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
    stages: &mut StageLog,
) -> Result<(CompileRequest, Vec<Diagnostic>), ApiError> {
    let local = prepare_local(state, fields, selected, target, &options, stages).await?;
    prepare_with_local(state, fields, options, local, stages).await
}

pub(crate) async fn prepare_local(
    state: &AppState,
    fields: &SubscriptionFields,
    selected: &[Uuid],
    target: Target,
    options: &PrepareOptions,
    stages: &mut StageLog,
) -> Result<PreparedInput, ApiError> {
    let mut target =
        Target::parse(&target.format).map_err(|error| ApiError::bad_request(error.to_string()))?;
    target.standalone = true;
    let profile = Profile {
        name: fields
            .remark
            .clone()
            .unwrap_or_else(|| "Subscription".into()),
        log_level: fields.log_level.clone(),
        editor: EditorConfig {
            rule_list: effective_default_on_empty(
                fields.use_system_rule_list,
                fields.rule_list.as_ref(),
                include_str!("toolbox_defaults/rules.jsonc"),
            )?,
            group: effective_default_on_empty(
                fields.use_system_group,
                fields.group.as_ref(),
                include_str!("toolbox_defaults/groups.jsonc"),
            )?,
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
    let custom_nodes = load_custom_nodes(state, selected, options.viewer, options.node_scope)
        .await
        .inspect_err(|error| {
            stages.push(json!({"type":"custom-nodes","status":"error","message":error.message()}));
        })?;
    stages.push(json!({"type":"custom-nodes","status":if selected.is_empty() { "skipped" } else { "ok" },"count":custom_nodes.len()}));
    Ok(PreparedInput {
        profile,
        custom_nodes,
        target,
    })
}

pub(crate) async fn prepare_with_local(
    state: &AppState,
    fields: &SubscriptionFields,
    options: PrepareOptions,
    local: PreparedInput,
    stages: &mut StageLog,
) -> Result<(CompileRequest, Vec<Diagnostic>), ApiError> {
    let PreparedInput {
        mut profile,
        custom_nodes,
        target,
    } = local;
    let mut snapshots = Vec::new();
    let mut diagnostics = Vec::new();
    let source_summary = load_sources(
        state,
        fields,
        &mut profile,
        options.cache_mode,
        stages,
        &mut snapshots,
        &mut diagnostics,
    )
    .await?;
    if source_summary.enabled > 0 && source_summary.failed == source_summary.enabled {
        let manual_count = if profile.editor.servers.trim().is_empty() {
            0
        } else {
            parse_jsonc_value(&profile.editor.servers)
                .map_err(|error| ApiError::bad_request(error.to_string()))?
                .as_array()
                .map_or(0, Vec::len)
        };
        if manual_count == 0 && custom_nodes.is_empty() {
            return Err(ApiError::unavailable(
                "all enabled subscription sources failed and no manual or assigned nodes are available",
            ));
        }
    }
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

fn debug_failure(mut stages: StageLog, stage: &str, message: &str) -> Value {
    if !stages.last().is_some_and(|value| {
        value.get("type") == Some(&json!(stage))
            && value.get("status") == Some(&json!("error"))
            && value.get("message") == Some(&json!(message))
    }) {
        stages.push(json!({"type":stage,"status":"error","message":message}));
    }
    let mut diagnostics = stages.iter().filter(|event| event.get("status").and_then(Value::as_str) == Some("error"))
        .map(|event| json!({"level":"error","sourceId":event.get("sourceId"),"message":event.get("message")}))
        .collect::<Vec<_>>();
    if diagnostics.is_empty() {
        diagnostics.push(json!({"level":"error","message":message}));
    }
    json!({"ok":false,"message":message,"diagnostics":diagnostics,"stages":stages.into_events()})
}

fn result_output(
    result: &CompileResult,
    stages: &StageLog,
    node_traces: &[Value],
    rule_samples: &[Value],
) -> Value {
    let decoded = serde_yaml::from_str::<Value>(&result.content).ok();
    json!({
        "ok": true,
        "format": result.format,
        "content": result.content,
        "decoded": decoded,
        "nodeOrigins": result.node_origins,
        "runtimeValidated": result.runtime_validated,
        "nodeCount": result.node_count,
        "diagnostics": result.diagnostics,
        "fieldDiffs": result.field_diffs,
        "nodeTraces": node_traces,
        "ruleSamples": rule_samples,
        "stages": &**stages
    })
}
