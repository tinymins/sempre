use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;

use crate::{VERSION, api::AppState};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/system", get(system))
        .route("/api/v1/system/network", get(network_inventory))
        .route("/api/v1/network/test", post(network_test))
}

async fn system(State(state): State<Arc<AppState>>) -> Response {
    let document = match state.manager.state() {
        Ok(document) => document,
        Err(error) => return internal(error.to_string()),
    };
    let web = match state.web.read() {
        Ok(web) => web,
        Err(error) => return internal(error.to_string()),
    };
    let layout = state.manager.store().layout();
    let ui_installed = sempre_ui::Store::new(&layout.ui).current().is_ok();
    let mode = match layout.mode {
        sempre_state::Mode::System => "system",
        sempre_state::Mode::Portable => "portable",
    };
    let selected = document.selected.as_ref().map(|selection| {
        json!({
            "core": selection.core,
            "repository": selection.repository,
            "ref": selection.reference,
        })
    });
    let active = document.active.as_ref().map(|deployment| {
        json!({
            "core": deployment.core,
            "repository": deployment.repository,
            "ref": deployment.reference,
            "version": deployment.version,
            "config_hash": deployment.config_hash,
        })
    });
    Json(json!({
        "version": VERSION,
        "commit": option_env!("SEMPRE_COMMIT").unwrap_or(""),
        "date": option_env!("SEMPRE_BUILD_DATE").unwrap_or(""),
        "mode": mode,
        "service": "unknown",
        "desired_state": document.desired_state,
        "runtime": document.runtime,
        "selected": selected,
        "active": active,
        "pending": document.pending,
        "last_error": document.last_error,
        "web": {
            "listen": web.listen,
            "local_url": state.local_url,
            "password_set": web.password_protected(),
            "password_warning": !web.password_protected(),
        },
        "ui": {
            "installed": ui_installed,
            "metadata": null,
        },
        "capabilities": {},
    }))
    .into_response()
}

async fn network_inventory() -> Response {
    match sempre_network::inventory() {
        Ok(inventory) => Json(inventory).into_response(),
        Err(error) => internal(error.to_string()),
    }
}

async fn network_test() -> Response {
    match sempre_network::run_network_test().await {
        Ok(report) => Json(report).into_response(),
        Err(error) => internal(error.to_string()),
    }
}

fn internal(message: impl Into<String>) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": { "code": "NETWORK_ERROR", "message": message.into() }
        })),
    )
        .into_response()
}
