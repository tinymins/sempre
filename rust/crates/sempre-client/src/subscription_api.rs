use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use sempre_converter::Profile;
use sempre_subscription::{SubscriptionError, new_profile};
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;

use crate::api::AppState;

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/subscriptions", get(list).post(create))
        .route(
            "/api/v1/subscriptions/{id}",
            get(get_profile).patch(rename).delete(remove),
        )
}

#[derive(Serialize)]
struct CatalogOutput {
    profiles: Vec<Profile>,
    custom_nodes: Vec<sempre_converter::CustomNode>,
    active_profile_id: Option<String>,
    interval: String,
    auto_restart: bool,
}

async fn list(State(state): State<Arc<AppState>>) -> Response {
    let catalog = match state.subscriptions.read() {
        Ok(catalog) => catalog,
        Err(error) => return internal(error.to_string()),
    };
    let document = match state.manager.state() {
        Ok(document) => document,
        Err(error) => return internal(error.to_string()),
    };
    Json(CatalogOutput {
        profiles: catalog.profiles,
        custom_nodes: catalog.custom_nodes,
        active_profile_id: document.active_profile_id,
        interval: document.subscription.interval,
        auto_restart: document.subscription_auto_restart,
    })
    .into_response()
}

#[derive(Deserialize)]
struct CreateInput {
    name: String,
    #[serde(default)]
    mode: String,
    manifest_url: Option<String>,
}

async fn create(State(state): State<Arc<AppState>>, Json(input): Json<CreateInput>) -> Response {
    let name = input.name.trim();
    if name.is_empty() {
        return bad_request("profile name is required");
    }
    let mut profile = new_profile(name);
    if input.mode == "remote" {
        let Some(manifest_url) = input.manifest_url else {
            return bad_request("remote profile requires a manifest URL");
        };
        if !valid_remote_url(&manifest_url) {
            return bad_request("remote manifest URL must be absolute HTTP(S) without credentials");
        }
        profile.extra.insert("mode".into(), json!("remote"));
        profile.extra.insert(
            "remote".into(),
            json!({ "manifest_url": manifest_url.trim() }),
        );
    } else if !input.mode.is_empty() && input.mode != "local" {
        return bad_request("profile mode must be local or remote");
    }
    let candidate = profile.clone();
    match state.subscriptions.update(|catalog| {
        catalog.profiles.push(candidate);
        Ok(())
    }) {
        Ok(_) => (StatusCode::CREATED, Json(profile)).into_response(),
        Err(error) => operation(error.to_string()),
    }
}

async fn get_profile(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.subscriptions.read() {
        Ok(catalog) => catalog
            .profiles
            .into_iter()
            .find(|profile| profile.id == id)
            .map_or_else(not_found, |profile| Json(profile).into_response()),
        Err(error) => internal(error.to_string()),
    }
}

#[derive(Deserialize)]
struct RenameInput {
    name: String,
}

async fn rename(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<RenameInput>,
) -> Response {
    let name = input.name.trim();
    if name.is_empty() {
        return bad_request("profile name is required");
    }
    let mut changed = None;
    let result = state.subscriptions.update(|catalog| {
        let profile = catalog
            .profiles
            .iter_mut()
            .find(|profile| profile.id == id)
            .ok_or_else(|| SubscriptionError::Invalid("profile was not found".into()))?;
        profile.name = name.into();
        profile.revision += 1;
        changed = Some(profile.clone());
        Ok(())
    });
    match result {
        Ok(_) => Json(changed.expect("updated profile")).into_response(),
        Err(error) => operation(error.to_string()),
    }
}

async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let document = match state.manager.state() {
        Ok(document) => document,
        Err(error) => return internal(error.to_string()),
    };
    if document.active_profile_id.as_deref() == Some(&id) {
        return bad_request("the active subscription profile cannot be deleted");
    }
    match state.subscriptions.update(|catalog| {
        if catalog.profiles.len() == 1 {
            return Err(SubscriptionError::Invalid(
                "at least one subscription profile is required".into(),
            ));
        }
        let before = catalog.profiles.len();
        catalog.profiles.retain(|profile| profile.id != id);
        if catalog.profiles.len() == before {
            return Err(SubscriptionError::Invalid("profile was not found".into()));
        }
        Ok(())
    }) {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => operation(error.to_string()),
    }
}

fn valid_remote_url(value: &str) -> bool {
    Url::parse(value.trim()).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

fn internal(error: impl Into<String>) -> Response {
    error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "SUBSCRIPTION_ERROR",
        error,
    )
}

fn operation(error: impl Into<String>) -> Response {
    error_response(
        StatusCode::BAD_REQUEST,
        "SUBSCRIPTION_OPERATION_FAILED",
        error,
    )
}

fn bad_request(message: &'static str) -> Response {
    error_response(StatusCode::BAD_REQUEST, "INVALID_SUBSCRIPTION", message)
}

fn not_found() -> Response {
    error_response(
        StatusCode::NOT_FOUND,
        "SUBSCRIPTION_NOT_FOUND",
        "profile was not found",
    )
}

fn error_response(status: StatusCode, code: &'static str, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message.into() } })),
    )
        .into_response()
}
