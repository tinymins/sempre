use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;

use crate::api::{AppState, api_error};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/tunnels", get(status).put(update))
        .route("/api/v1/tunnels/install", post(install))
        .route("/api/v1/tunnels/{id}/{action}", post(action))
        .route("/api/v1/tunnels/{id}/log", get(log))
}

async fn status(State(state): State<Arc<AppState>>) -> Response {
    match state.manager.tunnel_status() {
        Ok(status) => Json(status).into_response(),
        Err(error) => failure("TUNNEL_STATUS_FAILED", &error),
    }
}

async fn update(
    State(state): State<Arc<AppState>>,
    Json(config): Json<sempre_tunnel::Config>,
) -> Response {
    match state.manager.update_tunnels(config).await {
        Ok((status, restart)) => Json(json!({
            "status": status,
            "core_restart_requested": restart
        }))
        .into_response(),
        Err(error) => failure("TUNNEL_UPDATE_FAILED", &error),
    }
}

async fn install(State(state): State<Arc<AppState>>) -> Response {
    match state.manager.install_tunnel_tool().await {
        Ok(status) => Json(status).into_response(),
        Err(error) => failure("TUNNEL_INSTALL_FAILED", &error),
    }
}

async fn action(
    State(state): State<Arc<AppState>>,
    Path((id, action)): Path<(String, String)>,
) -> Response {
    match state.manager.tunnel_action(&id, &action).await {
        Ok(status) => (
            StatusCode::ACCEPTED,
            Json(json!({ "action": action, "status": status })),
        )
            .into_response(),
        Err(error) => failure("TUNNEL_ACTION_FAILED", &error),
    }
}

async fn log(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.manager.tunnel_log(&id) {
        Ok(content) => Json(json!({ "content": content.trim() })).into_response(),
        Err(error) => failure("TUNNEL_LOG_FAILED", &error),
    }
}

fn failure(code: &'static str, error: &sempre_manager::ManagerError) -> Response {
    api_error(StatusCode::BAD_REQUEST, code, error.to_string())
}
