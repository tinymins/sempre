use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use serde::Deserialize;
use serde_json::Value;

use sempre_converter::convert_clash_rule_set;

use crate::{
    AppState,
    error::ApiError,
    source_cache::{self, CacheMode, SourceKind, SourceRequest},
};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/proxy/sing-box/convert/rule", get(rule_v11))
        .route("/api/proxy/sing-box/convert/rule/12", get(rule_v12))
        .route("/api/proxy/sing-box/convert/rule/13", get(rule_v13))
        .route("/api/proxy/sing-box/convert/rule/14", get(rule_v13))
}

#[derive(Deserialize)]
struct RuleQuery {
    url: String,
}

async fn rule_v11(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RuleQuery>,
) -> Result<Json<Value>, ApiError> {
    convert(&state, &query.url, 1).await
}

async fn rule_v12(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RuleQuery>,
) -> Result<Json<Value>, ApiError> {
    convert(&state, &query.url, 3).await
}

async fn rule_v13(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RuleQuery>,
) -> Result<Json<Value>, ApiError> {
    convert(&state, &query.url, 4).await
}

async fn convert(state: &AppState, url: &str, version: u8) -> Result<Json<Value>, ApiError> {
    if url.trim().is_empty() {
        return Err(ApiError::bad_request("url is required"));
    }
    let loaded = source_cache::load(
        state,
        SourceRequest {
            url,
            ua: "sempre-rule-set/1",
            fetch_mode: "auto",
            proxy: None,
            source_id: "public-rule-conversion",
            ttl_minutes: 60,
            mode: CacheMode::Global,
            kind: SourceKind::RuleSet,
        },
    )
    .await?;
    Ok(Json(convert_clash_rule_set(&loaded.content, version)))
}
