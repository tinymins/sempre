use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use ipnet::IpNet;
use serde::Deserialize;

use crate::api::{AppState, api_error};

#[derive(Deserialize)]
pub(crate) struct CheckInput {
    range: String,
}

pub(crate) async fn check(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CheckInput>,
) -> Response {
    let Ok(range) = input.range.trim().parse::<IpNet>() else {
        return api_error(
            StatusCode::BAD_REQUEST,
            "INVALID_FAKE_IP_RANGE",
            "Invalid FakeIP CIDR",
        );
    };
    match tokio::task::spawn_blocking(move || state.manager.check_fakeip_range(range)).await {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(error)) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "FAKE_IP_CHECK_FAILED",
            error,
        ),
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "FAKE_IP_CHECK_FAILED",
            error.to_string(),
        ),
    }
}
