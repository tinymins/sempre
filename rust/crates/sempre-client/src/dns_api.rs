use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;

use crate::api::{AppState, api_error};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/dns/settings", get(settings).put(update_settings))
        .route("/api/v1/dns/queries", get(queries).delete(clear_queries))
}

async fn queries(State(state): State<Arc<AppState>>) -> Response {
    Json(json!({ "queries": state.manager.dns_queries() })).into_response()
}

async fn clear_queries(State(state): State<Arc<AppState>>) -> Response {
    match state.manager.clear_dns_queries() {
        Ok(()) => Json(json!({ "changed": true })).into_response(),
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DNS_QUERY_CLEAR_FAILED",
            error.to_string(),
        ),
    }
}

async fn settings(State(state): State<Arc<AppState>>) -> Response {
    Json(json!({
        "settings": state.manager.dns_settings(),
        "status": state.manager.dns_frontend_status(),
    }))
    .into_response()
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<sempre_manager::DnsSettings>,
) -> Response {
    match state.manager.update_dns_settings(settings).await {
        Ok((change, settings)) => Json(json!({
            "change": change,
            "settings": settings,
            "status": state.manager.dns_frontend_status(),
        }))
        .into_response(),
        Err(error) => api_error(
            StatusCode::BAD_REQUEST,
            "DNS_SETTINGS_UPDATE_FAILED",
            error.to_string(),
        ),
    }
}
