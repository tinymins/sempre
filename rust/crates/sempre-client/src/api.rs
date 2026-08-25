use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use sempre_control::{API_MAJOR, AuthStore, WebConfigStore, token_matches};
use sempre_manager::{MAX_CONFIG_SIZE, Manager};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::VERSION;
use crate::listener::{EndpointStore, RebindHandle};

const DAEMON_TOKEN_HEADER: &str = "x-sempre-daemon-token";

pub(crate) struct AppState {
    pub(crate) manager: Arc<Manager>,
    pub(crate) web: WebConfigStore,
    pub(crate) auth: AuthStore,
    daemon_token: String,
    pub(crate) endpoint: EndpointStore,
    pub(crate) rebind: Option<RebindHandle>,
}

impl AppState {
    pub(crate) fn new(
        manager: Arc<Manager>,
        web: WebConfigStore,
        daemon_token: String,
        bind: String,
        local_url: String,
    ) -> Self {
        Self {
            manager,
            web,
            auth: AuthStore::default(),
            daemon_token,
            endpoint: EndpointStore::new(bind, local_url),
            rebind: None,
        }
    }

    pub(crate) fn attach_rebind(&mut self, rebind: RebindHandle) {
        self.rebind = Some(rebind);
    }
}

pub(crate) fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/auth/login", post(login))
        .route(
            "/api/v1/configs/current",
            get(config_get).put(config_write_removed),
        )
        .route(
            "/api/v1/configs/validate",
            post(config_validate).layer(DefaultBodyLimit::max(MAX_CONFIG_SIZE + (64 << 10))),
        )
        .merge(crate::subscription_api::router())
        .merge(crate::core_management_api::router())
        .merge(crate::subscription_debug_api::router())
        .merge(crate::subscription_profile_debug_api::router())
        .merge(crate::subscription_tools_api::router())
        .merge(crate::custom_node_api::router())
        .merge(crate::runtime_api::router())
        .merge(crate::runtime_control_api::router())
        .merge(crate::runtime_events_api::router())
        .merge(crate::system_api::router())
        .merge(crate::web_ui_api::router())
        .layer(middleware::from_fn_with_state(state.clone(), security))
        .fallback(crate::web_ui_api::static_file)
        .with_state(state)
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
    api_major: u32,
    bind: String,
    local_url: String,
    runtime: sempre_state::RuntimeState,
    password_required: bool,
}

async fn health(State(state): State<Arc<AppState>>) -> Response {
    let document = match state.manager.state() {
        Ok(document) => document,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "STATE_ERROR",
                error.to_string(),
            );
        }
    };
    let password_required = match state.web.read() {
        Ok(config) => config.password_protected(),
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "CONFIG_ERROR",
                error.to_string(),
            );
        }
    };
    let endpoint = state.endpoint.get();
    Json(Health {
        status: "ok",
        version: VERSION,
        api_major: API_MAJOR,
        bind: endpoint.bind,
        local_url: endpoint.local_url,
        runtime: document.runtime.state,
        password_required,
    })
    .into_response()
}

#[derive(Deserialize)]
struct LoginInput {
    #[serde(default)]
    password: String,
}

#[derive(Serialize)]
struct LoginOutput {
    token: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    password_required: bool,
}

async fn login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    Json(input): Json<LoginInput>,
) -> Response {
    if !state.auth.allow_login(&remote.ip().to_string()) {
        return api_error(
            StatusCode::TOO_MANY_REQUESTS,
            "LOGIN_RATE_LIMITED",
            "too many login attempts",
        );
    }
    if input.password.len() > 1024 {
        return api_error(
            StatusCode::BAD_REQUEST,
            "INVALID_PASSWORD",
            "password is too long",
        );
    }
    let config = match state.web.read() {
        Ok(config) => config,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "CONFIG_ERROR",
                error.to_string(),
            );
        }
    };
    let password_required = config.password_protected();
    let password = input.password;
    let valid = match tokio::task::spawn_blocking(move || config.verify_password(&password)).await {
        Ok(valid) => valid,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "AUTH_ERROR",
                error.to_string(),
            );
        }
    };
    if !valid {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "INVALID_CREDENTIALS",
            "administrator password is incorrect",
        );
    }
    let session = state.auth.issue();
    Json(LoginOutput {
        token: session.token,
        expires_at: session.expires_at,
        password_required,
    })
    .into_response()
}

async fn config_get(State(state): State<Arc<AppState>>) -> Response {
    match state.manager.current_config() {
        Ok(config) => Json(config).into_response(),
        Err(error) => api_error(
            StatusCode::BAD_REQUEST,
            "CONFIG_UNAVAILABLE",
            error.to_string(),
        ),
    }
}

async fn config_write_removed() -> Response {
    api_error(
        StatusCode::GONE,
        "DIRECT_CONFIG_REMOVED",
        "generated configurations are read-only; edit a subscription profile instead",
    )
}

#[derive(Deserialize)]
struct ConfigValidateInput {
    content: String,
}

#[derive(Serialize)]
struct ConfigValidateOutput {
    valid: bool,
}

async fn config_validate(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ConfigValidateInput>,
) -> Response {
    match state
        .manager
        .validate_config_content(input.content.as_bytes())
        .await
    {
        Ok(()) => Json(ConfigValidateOutput { valid: true }).into_response(),
        Err(error) => api_error(
            StatusCode::BAD_REQUEST,
            "CONFIG_VALIDATION_FAILED",
            error.to_string(),
        ),
    }
}

async fn security(
    State(state): State<Arc<AppState>>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let origin = match allowed_origin(&state, &request) {
        Ok(origin) => origin,
        Err(error) => {
            return api_error(StatusCode::FORBIDDEN, "ORIGIN_NOT_ALLOWED", error.message);
        }
    };
    if request.method() == Method::OPTIONS {
        return add_cors(StatusCode::NO_CONTENT.into_response(), origin.as_ref());
    }
    let public = matches!(
        request.uri().path(),
        "/api/v1/health" | "/api/v1/auth/login"
    );
    if !public && !authenticated(&state, request.headers(), remote) {
        return add_cors(
            api_error(
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "a valid administrator session is required",
            ),
            origin.as_ref(),
        );
    }
    add_cors(next.run(request).await, origin.as_ref())
}

fn authenticated(state: &AppState, headers: &HeaderMap, remote: SocketAddr) -> bool {
    let daemon = headers
        .get(DAEMON_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|token| {
            remote.ip().is_loopback() && token_matches(token, &state.daemon_token)
        });
    if daemon {
        return true;
    }
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| state.auth.valid(token))
}

struct OriginRejection {
    message: &'static str,
}

fn allowed_origin(
    state: &AppState,
    request: &Request,
) -> Result<Option<HeaderValue>, OriginRejection> {
    let Some(origin) = request.headers().get(header::ORIGIN) else {
        return Ok(None);
    };
    let Ok(origin_text) = origin.to_str() else {
        return Err(OriginRejection {
            message: "invalid request origin",
        });
    };
    let allowed = same_origin(request.headers(), origin_text)
        || state
            .web
            .read()
            .is_ok_and(|config| config.password_protected());
    if allowed {
        Ok(Some(origin.clone()))
    } else {
        Err(OriginRejection {
            message: "cross-origin access requires an administrator password",
        })
    }
}

fn same_origin(headers: &HeaderMap, origin: &str) -> bool {
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Ok(origin) = Url::parse(origin) else {
        return false;
    };
    let Ok(request) = Url::parse(&format!("http://{host}")) else {
        return false;
    };
    origin.scheme() == "http"
        && origin.host() == request.host()
        && origin.port_or_known_default() == request.port_or_known_default()
        && origin.path() == "/"
        && origin.query().is_none()
        && origin.fragment().is_none()
}

fn add_cors(mut response: Response, origin: Option<&HeaderValue>) -> Response {
    let Some(origin) = origin else {
        return response;
    };
    let headers = response.headers_mut();
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.clone());
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(
            "Authorization, Content-Type, X-Sempre-Daemon-Token, X-Sempre-UI-Name",
        ),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE, OPTIONS"),
    );
    response
}

#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

pub(crate) fn api_error(
    status: StatusCode,
    code: &'static str,
    message: impl Into<String>,
) -> Response {
    (
        status,
        Json(ErrorEnvelope {
            error: ErrorBody {
                code,
                message: message.into(),
            },
        }),
    )
        .into_response()
}

#[cfg(test)]
mod custom_node_tests;
#[cfg(test)]
mod system_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod web_ui_tests;
