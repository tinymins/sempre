use std::sync::Arc;

use axum::{Json, Router, extract::State, response::IntoResponse, routing::post};
use serde::Deserialize;
use serde_json::json;

use crate::api::AppState;

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/network/diagnostics", post(run))
}

#[derive(Deserialize)]
struct DiagnosticInput {
    target: String,
}

async fn run(
    State(state): State<Arc<AppState>>,
    Json(input): Json<DiagnosticInput>,
) -> impl IntoResponse {
    let runtime_running = state
        .manager
        .runtime_status()
        .is_ok_and(|status| status.runtime_state == sempre_state::RuntimeState::Running);
    match sempre_network::run_network_diagnostics(&input.target, runtime_running).await {
        Ok(report) => Json(json!(report)).into_response(),
        Err(error) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": { "code": "NETWORK_DIAGNOSTIC_ERROR", "message": error.to_string() } })),
        )
            .into_response(),
    }
}
