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
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::time::{Duration, sleep};

use crate::{VERSION, api::AppState};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/system", get(system))
        .route("/api/v1/system/network", get(network_inventory))
        .route("/api/v1/network/test", post(network_test))
        .route("/api/v1/service/action", post(service_action))
}

async fn system(State(state): State<Arc<AppState>>) -> Response {
    let document = match state.manager.state() {
        Ok(document) => document,
        Err(error) => return internal(error.to_string()),
    };
    let private_access = match state.manager.private_access_status() {
        Ok(status) => status,
        Err(error) => return internal(error.to_string()),
    };
    let web = match state.web.read() {
        Ok(web) => web,
        Err(error) => return internal(error.to_string()),
    };
    let layout = state.manager.store().layout();
    let ui_installed = sempre_ui::Store::new(&layout.ui).current().is_ok();
    let endpoint = state.endpoint.get();
    let service = if layout.mode == sempre_state::Mode::Development {
        sempre_service::State::NotInstalled
    } else {
        sempre_service::status()
            .await
            .unwrap_or(sempre_service::State::Unknown)
    };
    let mode = match layout.mode {
        sempre_state::Mode::System => "system",
        sempre_state::Mode::Portable => "portable",
        sempre_state::Mode::Development => "development",
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
        "service_memory": current_process_memory(),
        "service": service,
        "desired_state": document.desired_state,
        "runtime": document.runtime,
        "selected": selected,
        "active": active,
        "pending": document.pending,
        "last_error": document.last_error,
        "private_access": private_access,
        "web": {
            "listen": web.listen,
            "local_url": endpoint.local_url,
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

fn current_process_memory() -> u64 {
    let pid = Pid::from_u32(std::process::id());
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    system.process(pid).map_or(0, sysinfo::Process::memory)
}

#[derive(Deserialize)]
struct ServiceActionInput {
    action: String,
}

async fn service_action(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ServiceActionInput>,
) -> Response {
    let action = match sempre_service::Action::parse(&input.action) {
        Ok(action) => action,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": { "code": "INVALID_SERVICE_ACTION", "message": error.to_string() }
                })),
            )
                .into_response();
        }
    };
    if state.manager.store().layout().mode == sempre_state::Mode::Development {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": {
                    "code": "SERVICE_UNAVAILABLE",
                    "message": "system service operations are unavailable in development mode"
                }
            })),
        )
            .into_response();
    }
    match sempre_service::status().await {
        Ok(sempre_service::State::NotInstalled) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": { "code": "SERVICE_NOT_INSTALLED", "message": "system service is not installed" }
                })),
            )
                .into_response();
        }
        Err(error) => return internal(error.to_string()),
        Ok(_) => {}
    }
    tokio::spawn(async move {
        sleep(Duration::from_millis(250)).await;
        let _ = sempre_service::action(action).await;
    });
    (
        StatusCode::ACCEPTED,
        Json(json!({ "status": "scheduled", "action": input.action })),
    )
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
