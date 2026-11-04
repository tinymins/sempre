use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use sempre_manager::{FetchProxyInput, ManagerError, ResourceRules};
use serde::Deserialize;
use serde_json::json;

use crate::api::{AppState, api_error};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/fetch-proxies", get(list).post(create))
        .route(
            "/api/v1/fetch-proxies/rules",
            get(read_rules).put(update_rules),
        )
        .route("/api/v1/fetch-proxies/rules/test", post(test_rules))
        .route(
            "/api/v1/fetch-proxies/{id}",
            get(read).put(update).delete(remove),
        )
        .route("/api/v1/fetch-proxies/{id}/test", post(test))
}

async fn list(State(state): State<Arc<AppState>>) -> Response {
    match state.manager.fetch_proxies() {
        Ok(items) => Json(json!({ "items": items })).into_response(),
        Err(error) => failure(&error),
    }
}

async fn read(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.manager.fetch_proxy(&id) {
        Ok(proxy) => Json(proxy).into_response(),
        Err(error) => failure(&error),
    }
}

async fn create(
    State(state): State<Arc<AppState>>,
    Json(input): Json<FetchProxyInput>,
) -> Response {
    match state.manager.create_fetch_proxy(input) {
        Ok(proxy) => (StatusCode::CREATED, Json(proxy)).into_response(),
        Err(error) => failure(&error),
    }
}

async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<FetchProxyInput>,
) -> Response {
    match state.manager.update_fetch_proxy(&id, input) {
        Ok(proxy) => Json(proxy).into_response(),
        Err(error) => failure(&error),
    }
}

async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.manager.remove_fetch_proxy(&id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => failure(&error),
    }
}

async fn test(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.manager.test_fetch_proxy(&id).await {
        Ok(elapsed) => Json(json!({ "elapsed_ms": elapsed })).into_response(),
        Err(error) => failure(&error),
    }
}

async fn read_rules(State(state): State<Arc<AppState>>) -> Response {
    match state.manager.resource_rules() {
        Ok(rules) => Json(rules).into_response(),
        Err(error) => failure(&error),
    }
}

async fn update_rules(
    State(state): State<Arc<AppState>>,
    Json(rules): Json<ResourceRules>,
) -> Response {
    let outdated = state
        .manager
        .resource_rules()
        .is_ok_and(|current| current.revision != rules.revision);
    if outdated {
        return api_error(
            StatusCode::CONFLICT,
            "PROXY_RULES_STALE",
            "proxy rules changed elsewhere; reload them first".to_owned(),
        );
    }
    match state.manager.update_resource_rules(rules) {
        Ok(rules) => Json(rules).into_response(),
        Err(error) => failure(&error),
    }
}

#[derive(Deserialize)]
struct RuleTest {
    url: String,
}

async fn test_rules(State(state): State<Arc<AppState>>, Json(input): Json<RuleTest>) -> Response {
    match state.manager.test_resource_rules(&input.url).await {
        Ok(result) => Json(result).into_response(),
        Err(error) => failure(&error),
    }
}

fn failure(error: &ManagerError) -> Response {
    let status = match error {
        ManagerError::InvalidOperation(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    api_error(status, "FETCH_PROXY_FAILED", error.to_string())
}
