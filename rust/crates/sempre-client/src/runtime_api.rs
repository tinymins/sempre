use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::{AppState, api_error};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/runtime/status", get(status))
        .route("/api/v1/runtime/start", post(start))
        .route("/api/v1/runtime/stop", post(stop))
        .route("/api/v1/runtime/restart", post(restart).get(restart_task))
        .route("/api/v1/runtime/restart/config", get(restart_config))
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
    match state.manager.start_restart_task() {
        Ok(task) => (
            StatusCode::ACCEPTED,
            Json(json!({
                "action": "restart", "task": task, "status": state.manager.runtime_status().ok(),
            })),
        )
            .into_response(),
        Err(error) => api_error(
            StatusCode::CONFLICT,
            error
                .runtime_action_code()
                .unwrap_or("RUNTIME_ACTION_FAILED"),
            error.to_string(),
        ),
    }
}

async fn restart_task(State(state): State<Arc<AppState>>) -> Response {
    Json(json!({ "task": state.manager.restart_task() })).into_response()
}

#[derive(Deserialize)]
struct RestartConfigQuery {
    id: String,
}

async fn restart_config(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RestartConfigQuery>,
) -> Response {
    match state.manager.restart_task_config(&query.id) {
        Some(config) => Json(config).into_response(),
        None => api_error(
            StatusCode::NOT_FOUND,
            "RESTART_CONFIG_NOT_FOUND",
            "Configuration for this restart task is unavailable",
        ),
    }
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
    if state
        .manager
        .restart_task()
        .is_some_and(|task| task.state == "running")
    {
        return api_error(
            StatusCode::CONFLICT,
            "RUNTIME_RESTART_IN_PROGRESS",
            "a core restart is already in progress",
        );
    }
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
