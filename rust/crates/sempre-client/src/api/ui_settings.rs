use std::{fs, io, sync::Arc};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};

use crate::api::{AppState, api_error};

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum UiMode {
    Simple,
    #[default]
    Advanced,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UiSettings {
    ui_mode: UiMode,
    #[serde(default)]
    friendly_name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UiSettingsUpdate {
    ui_mode: Option<UiMode>,
    friendly_name: Option<String>,
}

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/ui/settings", get(read).put(write))
}

fn load(state: &AppState) -> io::Result<UiSettings> {
    let path = state.manager.store().layout().home.join("ui-settings.json");
    match fs::read(path) {
        Ok(data) => serde_json::from_slice(&data).map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(UiSettings::default()),
        Err(error) => Err(error),
    }
}

pub(super) fn friendly_name(state: &AppState) -> io::Result<String> {
    Ok(load(state)?.friendly_name)
}

async fn read(State(state): State<Arc<AppState>>) -> Response {
    match load(&state) {
        Ok(settings) => Json(settings).into_response(),
        Err(error) => read_error(&error),
    }
}

fn read_error(error: &io::Error) -> Response {
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "UI_SETTINGS_READ_FAILED",
        error.to_string(),
    )
}

async fn write(
    State(state): State<Arc<AppState>>,
    Json(update): Json<UiSettingsUpdate>,
) -> Response {
    let _guard = state.ui_settings_lock.lock().expect("UI settings lock");
    let mut settings = match load(&state) {
        Ok(settings) => settings,
        Err(error) => return read_error(&error),
    };
    if let Some(mode) = update.ui_mode {
        settings.ui_mode = mode;
    }
    if let Some(name) = update.friendly_name {
        name.trim().clone_into(&mut settings.friendly_name);
    }
    let path = state.manager.store().layout().home.join("ui-settings.json");
    let mut data = serde_json::to_vec_pretty(&settings).expect("serialize UI settings");
    data.push(b'\n');
    match sempre_state::write_atomic(&path, &data, 0o600) {
        Ok(()) => Json(settings).into_response(),
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "UI_SETTINGS_WRITE_FAILED",
            error.to_string(),
        ),
    }
}
