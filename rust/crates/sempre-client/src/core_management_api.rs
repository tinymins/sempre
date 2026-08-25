use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;

use crate::api::{AppState, api_error};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/cores", get(cores))
        .route("/api/v1/cores/install", post(install))
        .route("/api/v1/cores/update", post(update))
        .route("/api/v1/cores/use", post(select))
        .route("/api/v1/cores/remove", post(remove))
        .route("/api/v1/cores/auto/diagnose", post(auto_diagnose))
        .route("/api/v1/cores/auto/apply", post(auto_apply))
}

#[derive(Deserialize)]
struct CoreInput {
    #[serde(default)]
    reference: String,
}

async fn cores(State(state): State<Arc<AppState>>) -> Response {
    match state.manager.core_inventory() {
        Ok(inventory) => Json(inventory).into_response(),
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CORE_INVENTORY_ERROR",
            error.to_string(),
        ),
    }
}

async fn install(State(state): State<Arc<AppState>>, Json(input): Json<CoreInput>) -> Response {
    match state.manager.install_core(&input.reference).await {
        Ok(result) => Json(result).into_response(),
        Err(error) => api_error(
            StatusCode::BAD_REQUEST,
            "CORE_INSTALL_FAILED",
            error.to_string(),
        ),
    }
}

async fn update(State(state): State<Arc<AppState>>, Json(input): Json<CoreInput>) -> Response {
    match state.manager.update_cores(&input.reference).await {
        Ok(changes) => Json(json!({ "changes": changes })).into_response(),
        Err(error) => api_error(
            StatusCode::BAD_REQUEST,
            "CORE_UPDATE_FAILED",
            error.to_string(),
        ),
    }
}

async fn select(State(state): State<Arc<AppState>>, Json(input): Json<CoreInput>) -> Response {
    match state.manager.select_core(&input.reference).await {
        Ok(change) => Json(change).into_response(),
        Err(error) => api_error(
            StatusCode::BAD_REQUEST,
            "CORE_SELECTION_FAILED",
            error.to_string(),
        ),
    }
}

async fn remove(State(state): State<Arc<AppState>>, Json(input): Json<CoreInput>) -> Response {
    match state.manager.remove_core(&input.reference) {
        Ok(change) => Json(change).into_response(),
        Err(error) => api_error(
            StatusCode::BAD_REQUEST,
            "CORE_REMOVE_FAILED",
            error.to_string(),
        ),
    }
}

async fn auto_diagnose(State(state): State<Arc<AppState>>) -> Response {
    match state.manager.diagnose_core_configuration() {
        Ok(report) => Json(report).into_response(),
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "AUTO_CONFIG_DIAGNOSIS_FAILED",
            error.to_string(),
        ),
    }
}

#[derive(Deserialize)]
struct AutoApplyInput {
    #[serde(default)]
    candidate_id: String,
}

async fn auto_apply(
    State(state): State<Arc<AppState>>,
    Json(input): Json<AutoApplyInput>,
) -> Response {
    match state
        .manager
        .apply_core_configuration(&input.candidate_id)
        .await
    {
        Ok(result) => Json(result).into_response(),
        Err(error) => api_error(
            StatusCode::BAD_REQUEST,
            "AUTO_CONFIG_APPLY_FAILED",
            error.to_string(),
        ),
    }
}
