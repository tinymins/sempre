use std::{path::Component, sync::Arc};

use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{DefaultBodyLimit, Query, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::json;
use tower::ServiceExt as _;
use tower_http::services::ServeFile;

use crate::api::AppState;

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/web", get(web_get).patch(web_patch))
        .route("/api/v1/ui", get(ui_get).delete(ui_remove))
        .route("/api/v1/ui/install", axum::routing::post(ui_install))
        .route(
            "/api/v1/ui/upload",
            axum::routing::post(ui_upload)
                .layer(DefaultBodyLimit::max(sempre_ui::MAX_ARCHIVE_SIZE)),
        )
        .route("/api/v1/ui/update", axum::routing::post(ui_update))
}

#[derive(Deserialize)]
struct WebPatch {
    listen: Option<String>,
    password: Option<String>,
}

async fn web_get(State(state): State<Arc<AppState>>) -> Response {
    let endpoint = state.endpoint.get();
    match state.web.read() {
        Ok(config) => Json(json!({
            "listen": endpoint.bind,
            "local_url": endpoint.local_url,
            "password_set": config.password_protected(),
            "password_warning": !config.password_protected(),
        }))
        .into_response(),
        Err(error) => internal(error.to_string()),
    }
}

async fn web_patch(State(state): State<Arc<AppState>>, Json(input): Json<WebPatch>) -> Response {
    if input.listen.is_none() && input.password.is_none() {
        return invalid("web configuration patch is empty");
    }
    let mut endpoint = state.endpoint.get();
    if let Some(listen) = input.listen.as_deref()
        && listen != endpoint.bind
    {
        let Some(rebind) = &state.rebind else {
            return error(
                StatusCode::CONFLICT,
                "WEB_REBIND_UNAVAILABLE",
                "web listener is not managed by this process",
            );
        };
        endpoint = match rebind.request(listen).await {
            Ok(endpoint) => endpoint,
            Err(error) => return operation(error),
        };
    }
    let reauthenticate = input.password.is_some();
    if let Some(password) = input.password {
        if password.len() > 1024 {
            return invalid("password is too long");
        }
        let web = state.web.clone();
        let update = tokio::task::spawn_blocking(move || web.set_password(&password)).await;
        if let Err(error) = update
            .map_err(|error| error.to_string())
            .and_then(|value| value.map_err(|error| error.to_string()))
        {
            return internal(error);
        }
        state.auth.invalidate_all();
    }
    let config = match state.web.read() {
        Ok(config) => config,
        Err(error) => return internal(error.to_string()),
    };
    Json(json!({
        "listen": endpoint.bind,
        "local_url": endpoint.local_url,
        "password_set": config.password_protected(),
        "reauthenticate": reauthenticate,
    }))
    .into_response()
}

async fn ui_get(State(state): State<Arc<AppState>>) -> Response {
    let store = sempre_ui::Store::new(&state.manager.store().layout().ui);
    match store.current() {
        Ok(metadata) => Json(json!({ "installed": true, "metadata": metadata })).into_response(),
        Err(sempre_ui::UiError::Read(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Json(json!({ "installed": false })).into_response()
        }
        Err(error) => operation(error.to_string()),
    }
}

#[derive(Deserialize)]
struct UiInstallInput {
    #[serde(default)]
    source: String,
    #[serde(default)]
    sha256: String,
}

async fn ui_install(
    State(state): State<Arc<AppState>>,
    Json(input): Json<UiInstallInput>,
) -> Response {
    let result = if input.source.is_empty() || input.source == "official" {
        install_official(&state).await
    } else {
        let store = ui_store(&state);
        store
            .install_url(&input.source, "url", &input.source, &input.sha256)
            .await
            .map_err(|error| error.to_string())
    };
    match result {
        Ok(metadata) => Json(metadata).into_response(),
        Err(error) => operation(error),
    }
}

#[derive(Deserialize)]
struct UploadQuery {
    #[serde(default)]
    sha256: String,
}

async fn ui_upload(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UploadQuery>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let data = match to_bytes(body, sempre_ui::MAX_ARCHIVE_SIZE).await {
        Ok(data) => data,
        Err(error) => return operation(error.to_string()),
    };
    let source = headers
        .get("x-sempre-ui-name")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("browser-upload.zip")
        .to_owned();
    let store = ui_store(&state);
    let digest = query.sha256;
    match tokio::task::spawn_blocking(move || store.install_bytes(&data, "local", &source, &digest))
        .await
    {
        Ok(Ok(metadata)) => Json(metadata).into_response(),
        Ok(Err(error)) => operation(error.to_string()),
        Err(error) => internal(error.to_string()),
    }
}

async fn ui_update(State(state): State<Arc<AppState>>) -> Response {
    let current = match ui_store(&state).current() {
        Ok(current) => current,
        Err(error) => return operation(error.to_string()),
    };
    let result = match current.source_type.as_str() {
        "official" => install_official(&state).await,
        "url" => ui_store(&state)
            .install_url(&current.source, "url", &current.source, "")
            .await
            .map_err(|error| error.to_string()),
        "local" => Err("locally uploaded UI has no update source; install another archive".into()),
        value => Err(format!("unsupported UI source type {value:?}")),
    };
    match result {
        Ok(metadata) => Json(metadata).into_response(),
        Err(error) => operation(error),
    }
}

async fn ui_remove(State(state): State<Arc<AppState>>) -> Response {
    let store = ui_store(&state);
    match tokio::task::spawn_blocking(move || store.remove()).await {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(error)) => operation(error.to_string()),
        Err(error) => internal(error.to_string()),
    }
}

async fn install_official(state: &AppState) -> Result<sempre_ui::Metadata, String> {
    let layout = state.manager.store().layout();
    let archive = layout.resources.join("sempre-ui.zip");
    if archive.is_file() {
        let digest = checksum(&layout.resources.join("SHA256SUMS"), "sempre-ui.zip")?;
        let store = ui_store(state);
        return tokio::task::spawn_blocking(move || {
            store.install_file(&archive, "official", "bundle", &digest)
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string());
    }
    let releases =
        sempre_artifact::GithubClient::new(concat!("Sempre/", env!("CARGO_PKG_VERSION")))
            .map_err(|error| error.to_string())?;
    let release = releases
        .release("tinymins/sempre", "stable")
        .await
        .map_err(|error| error.to_string())?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == "sempre-ui.zip")
        .ok_or_else(|| format!("release {} has no sempre-ui.zip", release.tag))?;
    let digest = asset
        .digest
        .parse::<sempre_artifact::Sha256Digest>()
        .map_err(|_| "official UI release asset has no valid SHA-256 digest".to_owned())?;
    ui_store(state)
        .install_url(&asset.url, "official", &asset.url, &digest.to_string())
        .await
        .map_err(|error| error.to_string())
}

fn ui_store(state: &AppState) -> sempre_ui::Store {
    sempre_ui::Store::new(&state.manager.store().layout().ui)
}

fn checksum(path: &std::path::Path, name: &str) -> Result<String, String> {
    let data = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    data.lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let digest = fields.next()?;
            let candidate = fields.next()?.trim_start_matches('*');
            (candidate == name
                && digest.len() == 64
                && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| digest.to_owned())
        })
        .ok_or_else(|| format!("{name} is absent from or invalid in SHA256SUMS"))
}

pub(crate) async fn static_file(State(state): State<Arc<AppState>>, request: Request) -> Response {
    if request.uri().path().starts_with("/api/v1/") {
        return error(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "API route was not found",
        );
    }
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return error(
            StatusCode::METHOD_NOT_ALLOWED,
            "METHOD_NOT_ALLOWED",
            "method is not allowed",
        );
    }
    let root = sempre_ui::Store::new(&state.manager.store().layout().ui).current_dir();
    let relative = request.uri().path().trim_start_matches('/');
    let requested = if relative.is_empty() {
        "index.html"
    } else {
        relative
    };
    if !safe_asset_path(requested) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let candidate = root.join(requested);
    let (target, index) = if candidate.is_file() {
        (candidate, requested == "index.html")
    } else {
        (root.join("index.html"), true)
    };
    if !target.is_file() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CACHE_CONTROL, "no-store")],
            "Sempre UI is not installed. Run: sempre ui install official\n",
        )
            .into_response();
    }
    let mut response = match ServeFile::new(target).oneshot(request).await {
        Ok(response) => response.map(Body::new),
        Err(error) => return internal(error.to_string()),
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(if index {
            "no-cache"
        } else {
            "public, max-age=31536000, immutable"
        }),
    );
    response
}

fn safe_asset_path(value: &str) -> bool {
    if value == sempre_ui::MANIFEST_NAME || value.contains('\\') {
        return false;
    }
    let path = std::path::Path::new(value);
    path.components().all(|component| match component {
        Component::Normal(name) => !name.to_string_lossy().starts_with('.'),
        _ => false,
    })
}

fn internal(message: impl Into<String>) -> Response {
    error(StatusCode::INTERNAL_SERVER_ERROR, "WEB_UI_ERROR", message)
}

fn operation(message: impl Into<String>) -> Response {
    error(StatusCode::BAD_REQUEST, "UI_OPERATION_FAILED", message)
}

fn invalid(message: impl Into<String>) -> Response {
    error(StatusCode::BAD_REQUEST, "INVALID_WEB_CONFIG", message)
}

fn error(status: StatusCode, code: &'static str, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message.into() } })),
    )
        .into_response()
}
