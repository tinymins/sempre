use std::sync::Arc;

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::StreamExt as _;
use serde::Deserialize;
use serde_json::json;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::io::AsyncWriteExt as _;
use tokio::time::{Duration, sleep};

use crate::{VERSION, api::AppState};

const MAX_UPDATE_ARCHIVE_SIZE: usize = 512 << 20;
const MAX_UPDATE_ARCHIVE_SIZE_U64: u64 = sempre_artifact::MAX_ARTIFACT_SIZE;

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/system", get(system))
        .route("/api/v1/system/network", get(network_inventory))
        .route("/api/v1/network/test", post(network_test))
        .route("/api/v1/service/action", post(service_action))
        .route(
            "/api/v1/service/update",
            get(service_update_check).post(service_update),
        )
        .route(
            "/api/v1/service/update/upload",
            post(service_update_upload).layer(DefaultBodyLimit::max(MAX_UPDATE_ARCHIVE_SIZE)),
        )
        .route(
            "/api/v1/service/update/settings",
            get(service_update_settings).put(update_service_update_settings),
        )
        .route("/api/v1/service/update/task", get(service_update_task))
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
    let network_automation = match state.manager.network_automation_status() {
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
        "network_automation": network_automation,
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

async fn service_update_check(State(state): State<Arc<AppState>>) -> Response {
    let settings = match read_service_update_settings(&state) {
        Ok(settings) => settings,
        Err(error) => return internal(error),
    };
    match crate::service_update::check(settings.allow_prerelease).await {
        Ok(status) => Json(status).into_response(),
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": { "code": "UPDATE_CHECK_FAILED", "message": error }
            })),
        )
            .into_response(),
    }
}

async fn service_update(State(state): State<Arc<AppState>>) -> Response {
    if state.manager.store().layout().mode != sempre_state::Mode::System {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": {
                    "code": "SERVICE_UNAVAILABLE",
                    "message": "Sempre updates require an installed system service"
                }
            })),
        )
            .into_response();
    }
    let settings = match read_service_update_settings(&state) {
        Ok(settings) => settings,
        Err(error) => return internal(error),
    };
    match crate::service_update::start(
        Arc::clone(&state.service_updates),
        Arc::clone(&state.manager),
        settings.allow_prerelease,
    ) {
        Ok(task) => (StatusCode::ACCEPTED, Json(json!({ "task": task }))).into_response(),
        Err(error) => (
            StatusCode::CONFLICT,
            Json(json!({
                "error": { "code": "UPDATE_IN_PROGRESS", "message": error }
            })),
        )
            .into_response(),
    }
}

async fn service_update_upload(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ServiceUpdateUploadQuery>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if state.manager.store().layout().mode != sempre_state::Mode::System {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": {
                    "code": "SERVICE_UNAVAILABLE",
                    "message": "Sempre updates require an installed system service"
                }
            })),
        )
            .into_response();
    }
    let name = match update_upload_name(&query.name) {
        Ok(name) => name,
        Err(error) => return update_upload_error(StatusCode::BAD_REQUEST, error),
    };
    let archive_format = match crate::service_update::uploaded_archive_format(&name) {
        Ok(format) => format,
        Err(error) => return update_upload_error(StatusCode::BAD_REQUEST, error),
    };
    let total = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if total > MAX_UPDATE_ARCHIVE_SIZE_U64 {
        return update_upload_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("Sempre update package exceeds {MAX_UPDATE_ARCHIVE_SIZE} bytes"),
        );
    }
    let settings = match read_service_update_settings(&state) {
        Ok(settings) => settings,
        Err(error) => return internal(error),
    };
    let temporary = match tempfile::Builder::new().prefix("sempre-update-").tempdir() {
        Ok(temporary) => temporary,
        Err(error) => return internal(format!("create update directory: {error}")),
    };
    let task = match state.service_updates.begin() {
        Ok(task) => task,
        Err(error) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": { "code": "UPDATE_IN_PROGRESS", "message": error }
                })),
            )
                .into_response();
        }
    };
    if let Err(error) = state.service_updates.set_upload(&task.id, &name, total) {
        state.service_updates.fail(&task.id, &error);
        return internal(error);
    }
    let archive = temporary.path().join("upload");
    if let Err(error) =
        receive_update_upload(body, &archive, &state.service_updates, &task.id, total).await
    {
        state.service_updates.fail(&task.id, &error);
        return update_upload_error(StatusCode::BAD_REQUEST, error);
    }
    crate::service_update::start_uploaded(
        Arc::clone(&state.service_updates),
        task.id.clone(),
        temporary,
        archive_format,
        settings.allow_prerelease,
    );
    let task = state.service_updates.snapshot().unwrap_or(task);
    (StatusCode::ACCEPTED, Json(json!({ "task": task }))).into_response()
}

#[derive(Deserialize)]
struct ServiceUpdateUploadQuery {
    #[serde(default)]
    name: String,
}

fn update_upload_name(value: &str) -> Result<String, String> {
    let name = value.trim();
    if name.is_empty() {
        return Err("Sempre update package name is missing".into());
    }
    if name.len() > 255 || name.contains('/') || name.contains('\\') {
        return Err("Sempre update package name is invalid".into());
    }
    Ok(name.to_owned())
}

async fn receive_update_upload(
    body: Body,
    archive: &std::path::Path,
    tasks: &crate::service_update_task::ServiceUpdateTasks,
    task_id: &str,
    total: u64,
) -> Result<(), String> {
    let mut file = tokio::fs::File::create(archive)
        .await
        .map_err(|error| format!("create uploaded update package: {error}"))?;
    let mut stream = body.into_data_stream();
    let mut uploaded = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("read uploaded update package: {error}"))?;
        uploaded = uploaded
            .checked_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX))
            .filter(|size| *size <= MAX_UPDATE_ARCHIVE_SIZE_U64)
            .ok_or_else(|| {
                format!("Sempre update package exceeds {MAX_UPDATE_ARCHIVE_SIZE} bytes")
            })?;
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("write uploaded update package: {error}"))?;
        tasks.upload_progress(task_id, uploaded, total);
    }
    if uploaded == 0 {
        return Err("Sempre update package is empty".into());
    }
    file.flush()
        .await
        .map_err(|error| format!("flush uploaded update package: {error}"))?;
    tasks.upload_progress(task_id, uploaded, uploaded);
    Ok(())
}

fn update_upload_error(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({
            "error": { "code": "UPDATE_UPLOAD_FAILED", "message": message.into() }
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
struct ServiceUpdateSettingsInput {
    allow_prerelease: bool,
}

async fn service_update_settings(State(state): State<Arc<AppState>>) -> Response {
    match read_service_update_settings(&state) {
        Ok(settings) => Json(json!({ "settings": settings })).into_response(),
        Err(error) => internal(error),
    }
}

async fn update_service_update_settings(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ServiceUpdateSettingsInput>,
) -> Response {
    let path = service_update_settings_path(&state);
    match crate::service_update_settings::write(&path, input.allow_prerelease) {
        Ok(settings) => Json(json!({ "settings": settings })).into_response(),
        Err(error) => internal(error),
    }
}

fn read_service_update_settings(
    state: &AppState,
) -> Result<crate::service_update_settings::ServiceUpdateSettings, String> {
    crate::service_update_settings::read(&service_update_settings_path(state))
}

fn service_update_settings_path(state: &AppState) -> std::path::PathBuf {
    state
        .manager
        .store()
        .layout()
        .home
        .join("service-update.json")
}

async fn service_update_task(State(state): State<Arc<AppState>>) -> Response {
    Json(json!({ "task": state.service_updates.snapshot() })).into_response()
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
