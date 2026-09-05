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
    Router::new().route(
        "/api/v1/network/settings",
        get(settings).put(update_settings),
    )
}

async fn settings(State(state): State<Arc<AppState>>) -> Response {
    let current = sempre_network::default_interface().unwrap_or_default();
    Json(json!({
        "settings": state.manager.network_settings(),
        "current": current,
        "platform": std::env::consts::OS,
        "gateway_available": cfg!(target_os = "linux"),
    }))
    .into_response()
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<sempre_manager::NetworkSettings>,
) -> Response {
    match state.manager.update_network_settings(settings).await {
        Ok((change, settings)) => Json(json!({
            "change": change,
            "settings": settings,
            "current": sempre_network::default_interface().unwrap_or_default(),
            "platform": std::env::consts::OS,
            "gateway_available": cfg!(target_os = "linux"),
        }))
        .into_response(),
        Err(error) => api_error(
            StatusCode::BAD_REQUEST,
            "NETWORK_SETTINGS_UPDATE_FAILED",
            error.to_string(),
        ),
    }
}
