use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Serialize;
use serde_json::json;

use crate::api::{AppState, api_error};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/runtime/status", get(status))
        .route("/api/v1/runtime/start", post(start))
        .route("/api/v1/runtime/stop", post(stop))
        .route("/api/v1/runtime/restart", post(restart))
        .route("/api/v1/runtime/reload", post(reload))
}

async fn status(State(state): State<Arc<AppState>>) -> Response {
    match state.manager.runtime_status() {
        Ok(status) => Json(status).into_response(),
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "RUNTIME_STATUS_FAILED",
            error.to_string(),
        ),
    }
}

async fn start(State(state): State<Arc<AppState>>) -> Response {
    action(state, "start").await
}

async fn stop(State(state): State<Arc<AppState>>) -> Response {
    action(state, "stop").await
}

async fn restart(State(state): State<Arc<AppState>>) -> Response {
    action(state, "restart").await
}

#[derive(Serialize)]
struct ActionOutput {
    action: &'static str,
    status: sempre_manager::RuntimeStatus,
}

async fn action(state: Arc<AppState>, action: &'static str) -> Response {
    match state.manager.runtime_action(action).await {
        Ok(status) => (StatusCode::ACCEPTED, Json(ActionOutput { action, status })).into_response(),
        Err(error) => {
            let Some(code) = error.runtime_action_code() else {
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "RUNTIME_ACTION_FAILED",
                    error.to_string(),
                );
            };
            let status = state.manager.runtime_status().ok();
            (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": {
                        "code": code,
                        "message": error.to_string(),
                        "details": { "status": status },
                    }
                })),
            )
                .into_response()
        }
    }
}

#[derive(Serialize)]
struct ReloadOutput {
    scheduled: bool,
    status: sempre_manager::RuntimeStatus,
}

async fn reload(State(state): State<Arc<AppState>>) -> Response {
    state.manager.request_runtime_reload();
    match state.manager.runtime_status() {
        Ok(status) => (
            StatusCode::ACCEPTED,
            Json(ReloadOutput {
                scheduled: true,
                status,
            }),
        )
            .into_response(),
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "RUNTIME_RELOAD_FAILED",
            error.to_string(),
        ),
    }
}
