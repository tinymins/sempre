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

use crate::api::{AppState, api_error};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/gateway", get(status).put(update))
        .route("/api/v1/gateway/validate", post(validate))
        .route("/api/v1/gateway/host-plan", post(host_plan))
        .route("/api/v1/gateway/host-apply", post(host_apply))
        .route("/api/v1/gateway/dns-query", post(dns_query))
        .route("/api/v1/gateway/leases/revoke", post(lease_revoke))
}

async fn status(State(state): State<Arc<AppState>>) -> Response {
    match state.manager.gateway_status().await {
        Ok(status) => Json(status).into_response(),
        Err(error) => failure("GATEWAY_STATUS_FAILED", &error),
    }
}

#[derive(Deserialize)]
struct DnsQueryInput {
    name: String,
    #[serde(rename = "type", default = "default_dns_type")]
    record_type: String,
}

fn default_dns_type() -> String {
    "A".into()
}

async fn dns_query(
    State(state): State<Arc<AppState>>,
    Json(input): Json<DnsQueryInput>,
) -> Response {
    match state
        .manager
        .gateway_dns_query(&input.name, &input.record_type)
        .await
    {
        Ok(result) => Json(result).into_response(),
        Err(error) => failure("GATEWAY_DNS_QUERY_FAILED", &error),
    }
}

#[derive(Deserialize)]
struct LeaseInput {
    mac: String,
}

async fn lease_revoke(
    State(state): State<Arc<AppState>>,
    Json(input): Json<LeaseInput>,
) -> Response {
    match state.manager.revoke_gateway_lease(&input.mac).await {
        Ok(()) => Json(json!({ "changed": true })).into_response(),
        Err(error) => failure("GATEWAY_LEASE_REVOKE_FAILED", &error),
    }
}

async fn update(
    State(state): State<Arc<AppState>>,
    Json(config): Json<sempre_gateway::Config>,
) -> Response {
    match state.manager.update_gateway(&config) {
        Ok((config, reload_requested)) => Json(json!({
            "config": config,
            "reload_requested": reload_requested
        }))
        .into_response(),
        Err(error) => failure("GATEWAY_UPDATE_FAILED", &error),
    }
}

async fn validate(Json(config): Json<sempre_gateway::Config>) -> Response {
    let errors = sempre_manager::Manager::<sempre_manager::ProcessRunner>::validate_gateway(config);
    Json(json!({ "valid": errors.is_empty(), "errors": errors })).into_response()
}

async fn host_plan(Json(input): Json<sempre_gateway::HostPlanRequest>) -> Response {
    match sempre_manager::Manager::<sempre_manager::ProcessRunner>::gateway_host_plan(input.config)
    {
        Ok(plan) => Json(plan).into_response(),
        Err(error) => failure("GATEWAY_HOST_PLAN_FAILED", &error),
    }
}

async fn host_apply(
    State(state): State<Arc<AppState>>,
    Json(input): Json<sempre_gateway::HostApplyRequest>,
) -> Response {
    match state.manager.apply_gateway_host_plan(input).await {
        Ok(plan) => Json(plan).into_response(),
        Err(error) => api_error(
            StatusCode::CONFLICT,
            "GATEWAY_HOST_APPLY_FAILED",
            error.to_string(),
        ),
    }
}

fn failure(code: &'static str, error: &sempre_manager::ManagerError) -> Response {
    api_error(StatusCode::BAD_REQUEST, code, error.to_string())
}
