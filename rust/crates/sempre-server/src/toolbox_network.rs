use std::{
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use axum::{Json, Router, routing::get};
use serde::Serialize;
use tokio::sync::RwLock;

use crate::{AppState, error::ApiError};

struct CacheEntry {
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

async fn geoip_cn() -> Result<Json<ListOutput>, ApiError> {
    list(&GEOIP, fetch_geoip()).await
}

async fn geosite_cn() -> Result<Json<ListOutput>, ApiError> {
    list(&GEOSITE, fetch_geosite()).await
}

async fn list(
    cache: &RwLock<Option<CacheEntry>>,
    fetch: impl std::future::Future<Output = Result<Vec<String>, ApiError>>,
) -> Result<Json<ListOutput>, ApiError> {
    if let Some(entry) = cache.read().await.as_ref()
        && entry.fetched_at.elapsed() < TTL
    {
        return Ok(Json(ListOutput {
            count: entry.items.len(),
            items: entry.items.clone(),
        }));
    }
    let items = fetch.await?;
    *cache.write().await = Some(CacheEntry {
        items: items.clone(),
        fetched_at: Instant::now(),
    });
    Ok(Json(ListOutput {
        count: items.len(),
        items,
    }))
}

async fn text(url: &str) -> Result<String, ApiError> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(ApiError::internal)?
        .get(url)
        .send()
        .await
        .map_err(|error| ApiError::unavailable(format!("network list fetch failed: {error}")))?;
    if !response.status().is_success() {
        return Err(ApiError::unavailable(format!(
            "network list source returned {}",
            response.status()
        )));
    }
    let body = response
        .bytes()
        .await
        .map_err(|error| ApiError::unavailable(format!("network list read failed: {error}")))?;
    if body.len() > 8 * 1024 * 1024 {
        return Err(ApiError::unavailable("network list source is too large"));
    }
    String::from_utf8(body.to_vec()).map_err(|_| ApiError::unavailable("network list is not UTF-8"))
}

fn lines(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

async fn fetch_geoip() -> Result<Vec<String>, ApiError> {
    const PRIMARY: &str =
        "https://raw.githubusercontent.com/17mon/china_ip_list/master/china_ip_list.txt";
    if let Ok(value) = text(PRIMARY).await {
        let items = lines(&value);
        if !items.is_empty() {
            return Ok(items);
        }
    }
    let value = text(APNIC).await?;
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

async fn fetch_geosite() -> Result<Vec<String>, ApiError> {
    const SOURCE: &str =
        "https://raw.githubusercontent.com/Loyalsoldier/v2ray-rules-dat/release/direct-list.txt";
    let items = lines(&text(SOURCE).await?);
    if items.is_empty() {
        return Err(ApiError::unavailable("no CN domains were found"));
    }
    Ok(items)
}
