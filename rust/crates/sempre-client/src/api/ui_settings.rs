use std::{fs, sync::Arc};

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
}

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/ui/settings", get(read).put(write))
}

async fn read(State(state): State<Arc<AppState>>) -> Response {
    let path = state.manager.store().layout().home.join("ui-settings.json");
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Json(UiSettings::default()).into_response();
        }
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "UI_SETTINGS_READ_FAILED",
                error.to_string(),
            );
        }
    };
    match serde_json::from_slice::<UiSettings>(&data) {
        Ok(settings) => Json(settings).into_response(),
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "UI_SETTINGS_READ_FAILED",
            error.to_string(),
        ),
    }
}

async fn write(State(state): State<Arc<AppState>>, Json(settings): Json<UiSettings>) -> Response {
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
