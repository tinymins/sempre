use std::{collections::BTreeMap, future::Future, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::AppState;

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/runtime/capabilities", get(capabilities))
        .route("/api/v1/runtime/overview", get(overview))
        .route("/api/v1/runtime/config", get(config).patch(config_patch))
        .route("/api/v1/runtime/proxies", get(proxies))
        .route("/api/v1/runtime/proxies/select", post(proxy_select))
        .route("/api/v1/runtime/proxies/delay", post(proxy_delay))
        .route("/api/v1/runtime/providers", get(providers))
        .route("/api/v1/runtime/providers/update", post(provider_update))
        .route(
            "/api/v1/runtime/providers/healthcheck",
            post(provider_healthcheck),
        )
        .route("/api/v1/runtime/rules", get(rules))
        .route("/api/v1/runtime/rule-providers", get(rule_providers))
        .route(
            "/api/v1/runtime/rule-providers/update",
            post(rule_provider_update),
        )
        .route("/api/v1/runtime/connections", get(connections))
        .route("/api/v1/runtime/connections/close", post(connection_close))
        .route("/api/v1/runtime/dns/query", post(dns_query))
        .route("/api/v1/runtime/cache/flush", post(cache_flush))
}

pub(crate) fn client(
    state: &AppState,
) -> Result<sempre_core_control::Client, sempre_core_control::ControlError> {
    sempre_core_control::Client::from_file(&state.manager.store().layout().core_control)
}

async fn overview(State(state): State<Arc<AppState>>) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    result(client.overview().await)
}

async fn config(State(state): State<Arc<AppState>>) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    result(client.config().await)
}

async fn config_patch(State(state): State<Arc<AppState>>, Json(patch): Json<Value>) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    operation(
        client.patch_config(patch).await,
        json!({ "updated": true, "persistent": false }),
    )
}

async fn proxies(State(state): State<Arc<AppState>>) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    result(client.proxies().await)
}

#[derive(Deserialize)]
struct ProxySelect {
    group: String,
    proxy: String,
}

async fn proxy_select(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ProxySelect>,
) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    operation(
        client.select_proxy(&input.group, &input.proxy).await,
        json!({ "selected": true }),
    )
}

#[derive(Deserialize)]
struct DelayInput {
    name: String,
    #[serde(default = "default_test_url")]
    url: String,
    #[serde(default = "default_timeout")]
    timeout: u64,
}

async fn proxy_delay(
    State(state): State<Arc<AppState>>,
    Json(input): Json<DelayInput>,
) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    result(
        client
            .proxy_delay(&input.name, &input.url, input.timeout)
            .await
            .map(|delay| json!({ "delay": delay })),
    )
}

async fn providers(State(state): State<Arc<AppState>>) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    result(client.providers().await)
}

#[derive(Deserialize)]
struct NameInput {
    name: String,
}

async fn provider_update(
    State(state): State<Arc<AppState>>,
    Json(input): Json<NameInput>,
) -> Response {
    provider_action(&state, input, false).await
}

async fn provider_healthcheck(
    State(state): State<Arc<AppState>>,
    Json(input): Json<NameInput>,
) -> Response {
    provider_action(&state, input, true).await
}

async fn provider_action(state: &AppState, input: NameInput, healthcheck: bool) -> Response {
    let client = match client(state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    operation(
        client.provider_action(&input.name, healthcheck).await,
        json!({ "updated": true }),
    )
}

async fn rules(State(state): State<Arc<AppState>>) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    result(client.rules().await)
}

async fn rule_providers(State(state): State<Arc<AppState>>) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    result(client.rule_providers().await)
}

async fn rule_provider_update(
    State(state): State<Arc<AppState>>,
    Json(input): Json<NameInput>,
) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    operation(
        client.update_rule_provider(&input.name).await,
        json!({ "updated": true }),
    )
}

async fn connections(State(state): State<Arc<AppState>>) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    result(client.connections().await)
}

#[derive(Deserialize)]
struct ConnectionInput {
    #[serde(default)]
    id: String,
}

async fn connection_close(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ConnectionInput>,
) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    operation(
        client.close_connection(&input.id).await,
        json!({ "closed": true }),
    )
}

#[derive(Deserialize)]
struct DnsInput {
    name: String,
    #[serde(rename = "type", default = "default_record_type")]
    record_type: String,
}

async fn dns_query(State(state): State<Arc<AppState>>, Json(input): Json<DnsInput>) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    result(client.dns_query(&input.name, &input.record_type).await)
}

async fn cache_flush(State(state): State<Arc<AppState>>) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    operation(client.flush_fake_ip().await, json!({ "flushed": true }))
}

async fn capabilities(State(state): State<Arc<AppState>>) -> Response {
    let client = match client(&state) {
        Ok(client) => client,
        Err(error) => return runtime_error(&error),
    };
    let (proxies, providers, rules, rule_providers, connections) = tokio::join!(
        probe(client.proxies()),
        probe(client.providers()),
        probe(client.rules()),
        probe(client.rule_providers()),
        probe(client.connections()),
    );
    let mut values = BTreeMap::from([
        ("overview", true),
        ("runtime-config", true),
        ("traffic", true),
        ("memory", true),
        ("logs", true),
        ("dns-query", true),
        ("reload", true),
        ("proxies", proxies),
        ("latency", proxies),
        ("providers", providers),
        ("provider-update", providers),
        ("rules", rules),
        ("rule-providers", rule_providers),
        ("connections", connections),
        ("connection-close", connections),
    ]);
    values.retain(|_, supported| *supported);
    Json(values).into_response()
}

async fn probe<T>(
    future: impl Future<Output = Result<T, sempre_core_control::ControlError>>,
) -> bool {
    tokio::time::timeout(Duration::from_secs(2), future)
        .await
        .is_ok_and(|result| result.is_ok())
}

fn result<T: serde::Serialize>(value: Result<T, sempre_core_control::ControlError>) -> Response {
    match value {
        Ok(value) => Json(value).into_response(),
        Err(error) => runtime_error(&error),
    }
}

fn operation(value: Result<(), sempre_core_control::ControlError>, response: Value) -> Response {
    value.map_or_else(
        |error| runtime_error(&error),
        |()| Json(response).into_response(),
    )
}

pub(crate) fn runtime_error(error: &sempre_core_control::ControlError) -> Response {
    let (status, code, details) = match &error {
        sempre_core_control::ControlError::Unavailable
        | sempre_core_control::ControlError::InvalidMetadata(_)
        | sempre_core_control::ControlError::UnsupportedProtocol { .. } => {
            (StatusCode::CONFLICT, "CORE_UNAVAILABLE", Value::Null)
        }
        sempre_core_control::ControlError::Status { status, body } => (
            StatusCode::BAD_GATEWAY,
            "CORE_API_ERROR",
            json!({ "status": status, "response": body }),
        ),
        _ => (StatusCode::BAD_GATEWAY, "CORE_UNAVAILABLE", Value::Null),
    };
    (
        status,
        Json(json!({
            "error": { "code": code, "message": error.to_string(), "details": details }
        })),
    )
        .into_response()
}

fn default_test_url() -> String {
    "https://www.gstatic.com/generate_204".into()
}

const fn default_timeout() -> u64 {
    5000
}

fn default_record_type() -> String {
    "A".into()
}
