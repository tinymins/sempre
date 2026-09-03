use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use sempre_converter::Profile;
use sempre_subscription::{SubscriptionError, new_profile};
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;

use crate::api::AppState;

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/subscription", patch(update_schedule))
        .route("/api/v1/subscriptions", get(list).post(create))
        .route("/api/v1/subscriptions/defaults", get(defaults))
        .route("/api/v1/subscriptions/cache/clear", post(clear_cache))
        .route(
            "/api/v1/subscriptions/{id}",
            get(get_profile).put(save).patch(rename).delete(remove),
        )
        .route("/api/v1/subscriptions/{id}/refresh", post(refresh_profile))
        .route(
            "/api/v1/subscriptions/{id}/activate",
            post(activate_profile),
        )
}

#[derive(Deserialize)]
struct ScheduleInput {
    interval: Option<String>,
    auto_restart: Option<bool>,
}

async fn update_schedule(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ScheduleInput>,
) -> Response {
    if input.interval.is_none() && input.auto_restart.is_none() {
        return bad_request("subscription schedule patch is empty");
    }
    match state
        .manager
        .update_subscription_settings(input.interval.as_deref(), input.auto_restart)
    {
        Ok(changes) => Json(json!({ "changes": changes })).into_response(),
        Err(error) => operation(error.to_string()),
    }
}

#[derive(Serialize)]
struct CatalogOutput {
    profiles: Vec<Profile>,
    custom_nodes: Vec<sempre_converter::CustomNode>,
    active_profile_id: Option<String>,
    schedule: sempre_state::Subscription,
    auto_restart: bool,
    targets: Vec<sempre_converter::Target>,
    defaults: sempre_converter::Defaults,
    editor_defaults: sempre_converter::EditorDefaults,
    configuration_context: sempre_manager::ConfigurationContext,
}

async fn list(State(state): State<Arc<AppState>>) -> Response {
    let catalog = match state.manager.subscriptions().read() {
        Ok(catalog) => catalog,
        Err(error) => return internal(error.to_string()),
    };
    let document = match state.manager.state() {
        Ok(document) => document,
        Err(error) => return internal(error.to_string()),
    };
    let configuration_context = match state.manager.configuration_context() {
        Ok(context) => context,
        Err(error) => return internal(error.to_string()),
    };
    Json(CatalogOutput {
        profiles: catalog.profiles,
        custom_nodes: catalog.custom_nodes,
        active_profile_id: document.active_profile_id,
        schedule: document.subscription,
        auto_restart: document.subscription_auto_restart,
        targets: sempre_converter::available_targets(),
        defaults: sempre_converter::system_defaults(),
        editor_defaults: sempre_converter::recommended_editor_defaults(),
        configuration_context,
    })
    .into_response()
}

async fn defaults() -> Response {
    let profile = new_profile("");
    Json(json!({
        "profile": profile,
        "defaults": sempre_converter::system_defaults(),
        "editor_defaults": sempre_converter::recommended_editor_defaults(),
        "targets": sempre_converter::available_targets(),
        "source_defaults": {
            "id": "", "type": "url", "enabled": true,
            "url": "", "remark": "", "prefix": "", "content": "",
            "user_agent": "clash.meta", "fetch_mode": "auto"
        }
    }))
    .into_response()
}

async fn clear_cache(State(state): State<Arc<AppState>>) -> Response {
    match state.manager.clear_subscription_cache() {
        Ok(change) => Json(change).into_response(),
        Err(error) => operation(error.to_string()),
    }
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
    match state.manager.subscriptions().update(|catalog| {
        if input.mode != "remote" {
            profile.custom_node_ids = catalog
                .custom_nodes
                .iter()
                .map(|node| node.id.clone())
                .collect();
        }
        catalog.profiles.push(profile.clone());
        Ok(())
    }) {
        Ok(_) => (StatusCode::CREATED, Json(profile)).into_response(),
        Err(error) => operation(error.to_string()),
    }
}

async fn get_profile(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.manager.subscriptions().read() {
        Ok(catalog) => catalog
            .profiles
            .into_iter()
            .find(|profile| profile.id == id)
            .map_or_else(not_found, |profile| Json(profile).into_response()),
        Err(error) => internal(error.to_string()),
    }
}

#[derive(Serialize)]
struct SaveOutput {
    change: sempre_manager::CoreChange,
    profile: Profile,
}

async fn save(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(candidate): Json<Profile>,
) -> Response {
    let expected = headers
        .get("x-sempre-configuration-context")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty());
    match state
        .manager
        .save_subscription_profile(&id, candidate, expected)
    {
        Ok((change, profile)) => Json(SaveOutput { change, profile }).into_response(),
        Err(sempre_manager::ManagerError::ConfigurationContextChanged) => error_response(
            StatusCode::CONFLICT,
            "CONFIGURATION_CONTEXT_CHANGED",
            "subscription configuration target changed; reload before saving",
        ),
        Err(error) => operation(error.to_string()),
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
    let result = state.manager.subscriptions().update(|catalog| {
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
    match state.manager.subscriptions().update(|catalog| {
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

#[derive(Serialize)]
struct PrepareOutput {
    change: sempre_manager::CoreChange,
    render: sempre_manager::SubscriptionRender,
}

async fn refresh_profile(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.manager.refresh_subscription_profile(&id).await {
        Ok((change, render)) => Json(PrepareOutput { change, render }).into_response(),
        Err(error) => operation(error.to_string()),
    }
}

async fn activate_profile(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.manager.activate_subscription_profile(&id).await {
        Ok((change, render)) => Json(PrepareOutput { change, render }).into_response(),
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

pub(crate) fn operation(error: impl Into<String>) -> Response {
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
