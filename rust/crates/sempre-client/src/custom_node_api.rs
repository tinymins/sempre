use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use sempre_converter::CustomNode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::AppState;

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/custom-nodes", get(list).post(create))
        .route(
            "/api/v1/custom-nodes/{id}",
            axum::routing::put(update).delete(remove),
        )
}

#[derive(Deserialize)]
struct CustomNodeInput {
    #[serde(default)]
    name: String,
    proxy: Value,
    // Request-only inverse view of Profile.custom_node_ids; never stored on a node.
    subscription_ids: Option<Vec<String>>,
}

async fn list(State(state): State<Arc<AppState>>) -> Response {
    match state.manager.custom_nodes() {
        Ok(nodes) => Json(json!({ "nodes": nodes })).into_response(),
        Err(error) => internal(error.to_string()),
    }
}

async fn create(
    State(state): State<Arc<AppState>>,
    Json(mut input): Json<CustomNodeInput>,
) -> Response {
    let subscriptions = input.subscription_ids.take();
    match state
        .manager
        .save_custom_node_with_subscriptions(candidate("", input), subscriptions.as_deref())
    {
        Ok(node) => (StatusCode::CREATED, Json(node)).into_response(),
        Err(error) => operation(error.to_string()),
    }
}

async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(mut input): Json<CustomNodeInput>,
) -> Response {
    let subscriptions = input.subscription_ids.take();
    match state
        .manager
        .save_custom_node_with_subscriptions(candidate(&id, input), subscriptions.as_deref())
    {
        Ok(node) => Json(node).into_response(),
        Err(error) => operation(error.to_string()),
    }
}

async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.manager.remove_custom_node(&id) {
        Ok(change) => Json(change).into_response(),
        Err(error) => operation(error.to_string()),
    }
}

fn candidate(id: &str, input: CustomNodeInput) -> CustomNode {
    CustomNode {
        id: id.into(),
        name: input.name,
        proxy: input.proxy,
        created_at: None,
        updated_at: None,
    }
}

fn internal(message: impl Into<String>) -> Response {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "CUSTOM_NODE_ERROR",
        message,
    )
}

fn operation(message: impl Into<String>) -> Response {
    error(
        StatusCode::BAD_REQUEST,
        "CUSTOM_NODE_OPERATION_FAILED",
        message,
    )
}

fn error(status: StatusCode, code: &'static str, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message.into() } })),
    )
        .into_response()
}
