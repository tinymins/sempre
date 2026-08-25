use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::Deserialize;
use serde_json::json;

use crate::{api::AppState, subscription_api::operation};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/subscriptions/{id}/render", post(render))
        .route("/api/v1/subscriptions/{id}/preview", post(render))
        .route(
            "/api/v1/subscriptions/{id}/preview-nodes",
            post(preview_nodes),
        )
        .route("/api/v1/subscriptions/{id}/trace", post(trace))
        .route("/api/v1/subscriptions/source/test", post(source_test))
}

#[derive(Deserialize)]
struct RenderInput {
    #[serde(default = "default_format")]
    format: String,
    #[serde(default)]
    force: bool,
}

async fn render(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<RenderInput>,
) -> Response {
    match state
        .manager
        .render_subscription_profile(&id, &input.format, input.force)
        .await
    {
        Ok(render) => Json(render).into_response(),
        Err(error) => operation(error.to_string()),
    }
}

#[derive(Deserialize)]
struct FormatInput {
    #[serde(default = "default_format")]
    format: String,
}

async fn preview_nodes(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<FormatInput>,
) -> Response {
    match state
        .manager
        .preview_subscription_nodes(&id, &input.format)
        .await
    {
        Ok(nodes) => Json(json!({ "nodes": nodes })).into_response(),
        Err(error) => operation(error.to_string()),
    }
}

#[derive(Deserialize)]
struct TraceInput {
    name: String,
    #[serde(default = "default_format")]
    format: String,
}

async fn trace(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<TraceInput>,
) -> Response {
    match state
        .manager
        .trace_subscription_node(&id, &input.name, &input.format)
        .await
    {
        Ok(trace) => Json(trace).into_response(),
        Err(error) => operation(error.to_string()),
    }
}

async fn source_test(
    State(state): State<Arc<AppState>>,
    Json(source): Json<sempre_converter::Source>,
) -> Response {
    match state.manager.test_subscription_source(source, true).await {
        Ok(result) => Json(result).into_response(),
        Err(error) => operation(error.to_string()),
    }
}

fn default_format() -> String {
    "sing-box-v13".into()
}
