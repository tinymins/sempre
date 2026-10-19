use std::{
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;
use tokio::sync::RwLock;

use crate::{AppState, error::ApiError, relay_egress::Policy, resource_relay};

struct CacheEntry {
    policy_identity: String,
    items: Vec<String>,
    fetched_at: Instant,
}

static GEOIP: LazyLock<RwLock<Option<CacheEntry>>> = LazyLock::new(|| RwLock::new(None));
static GEOSITE: LazyLock<RwLock<Option<CacheEntry>>> = LazyLock::new(|| RwLock::new(None));
const TTL: Duration = Duration::from_hours(24);
const APNIC: &str = "https://ftp.apnic.net/apnic/stats/apnic/delegated-apnic-latest";

#[derive(Serialize)]
struct ListOutput {
    count: usize,
    items: Vec<String>,
}

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/network/geoip/cn", get(geoip_cn))
        .route("/api/v1/network/geosite/cn", get(geosite_cn))
}

async fn geoip_cn(State(state): State<Arc<AppState>>) -> Result<Json<ListOutput>, ApiError> {
    let policy = Policy::load(&state).await?;
    let identity = policy.cache_identity(&state).await?;
    list(&GEOIP, identity, fetch_geoip(&state, &policy)).await
}

async fn geosite_cn(State(state): State<Arc<AppState>>) -> Result<Json<ListOutput>, ApiError> {
    let policy = Policy::load(&state).await?;
    let identity = policy.cache_identity(&state).await?;
    list(&GEOSITE, identity, fetch_geosite(&state, &policy)).await
}

async fn list(
    cache: &RwLock<Option<CacheEntry>>,
    policy_identity: String,
    fetch: impl std::future::Future<Output = Result<Vec<String>, ApiError>>,
) -> Result<Json<ListOutput>, ApiError> {
    if let Some(entry) = cache.read().await.as_ref()
        && entry.fetched_at.elapsed() < TTL
        && entry.policy_identity == policy_identity
    {
        return Ok(Json(ListOutput {
            count: entry.items.len(),
            items: entry.items.clone(),
        }));
    }
    let items = fetch.await?;
    *cache.write().await = Some(CacheEntry {
        policy_identity,
        items: items.clone(),
        fetched_at: Instant::now(),
    });
    Ok(Json(ListOutput {
        count: items.len(),
        items,
    }))
}

async fn text(state: &AppState, policy: &Policy, url: &str) -> Result<String, ApiError> {
    let response = resource_relay::fetch_with_policy_options(
        state,
        url,
        policy,
        resource_relay::FetchOptions {
            user_agent: "",
            max_size: 8 * 1024 * 1024,
            timeout: Duration::from_secs(30),
        },
    )
    .await?
    .response;
    if !(200..300).contains(&response.status) {
        return Err(ApiError::unavailable(format!(
            "network list source returned {}",
            response.status
        )));
    }
    String::from_utf8(response.content)
        .map_err(|_| ApiError::unavailable("network list is not UTF-8"))
}

fn lines(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

async fn fetch_geoip(state: &AppState, policy: &Policy) -> Result<Vec<String>, ApiError> {
    const PRIMARY: &str =
        "https://raw.githubusercontent.com/17mon/china_ip_list/master/china_ip_list.txt";
    if let Ok(value) = text(state, policy, PRIMARY).await {
        let items = lines(&value);
        if !items.is_empty() {
            return Ok(items);
        }
    }
    let value = text(state, policy, APNIC).await?;
    let items = value
        .lines()
        .filter_map(|line| {
            let parts = line.split('|').collect::<Vec<_>>();
            if parts.len() < 5 || parts[1] != "CN" || parts[2] != "ipv4" {
                return None;
            }
            let count = parts[4].parse::<u32>().ok()?;
            if count == 0 || !count.is_power_of_two() {
                return None;
            }
            Some(format!("{}/{}", parts[3], 32 - count.ilog2()))
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        return Err(ApiError::unavailable("no CN IPv4 ranges were found"));
    }
    Ok(items)
}

async fn fetch_geosite(state: &AppState, policy: &Policy) -> Result<Vec<String>, ApiError> {
    const SOURCE: &str =
        "https://raw.githubusercontent.com/Loyalsoldier/v2ray-rules-dat/release/direct-list.txt";
    let items = lines(&text(state, policy, SOURCE).await?);
    if items.is_empty() {
        return Err(ApiError::unavailable("no CN domains were found"));
    }
    Ok(items)
}
