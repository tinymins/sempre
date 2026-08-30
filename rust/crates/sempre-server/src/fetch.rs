use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use futures_util::StreamExt as _;
use reqwest::{Client, StatusCode, header};
use sempre_converter::{Profile, Source, SourceSnapshot, parse_subscription};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::Row as _;
use url::Url;
use uuid::Uuid;

use crate::{AppState, error::ApiError};

const MAX_SOURCE_SIZE: usize = 32 << 20;
const MAX_REDIRECTS: usize = 5;
const DEFAULT_USER_AGENT: &str = "clash.meta";

#[derive(Debug, Serialize)]
pub(crate) struct SourceTestResult {
    source_id: String,
    source_type: String,
    format: String,
    byte_count: usize,
    content_hash: String,
    node_count: usize,
    discarded_node_count: usize,
    diagnostics: Vec<String>,
}

pub(crate) async fn test_source(
    state: &AppState,
    source: &Source,
) -> Result<SourceTestResult, ApiError> {
    let content = match source.kind.as_str() {
        "raw" => source.content.clone(),
        "url" if !source.url.trim().is_empty() => {
            let user_agent = if source.user_agent.trim().is_empty() {
                DEFAULT_USER_AGENT
            } else {
                source.user_agent.trim()
            };
            let proxy = (source
                .extra
                .get("fetch_mode")
                .and_then(serde_json::Value::as_str)
                == Some("domestic-direct"))
            .then_some(state.config.direct_proxy_url.as_deref())
            .flatten();
            fetch_source_text(&source.url, user_agent, proxy).await?
        }
        _ => return Err(ApiError::bad_request("source is invalid")),
    };
    validate_content(&content)?;
    let parsed = parse_subscription(&content);
    Ok(SourceTestResult {
        source_id: source.id.clone(),
        source_type: source.kind.clone(),
        format: parsed.format,
        byte_count: content.len(),
        content_hash: format!("{:x}", Sha256::digest(content.as_bytes())),
        node_count: parsed.nodes.len(),
        discarded_node_count: parsed.discarded_placeholder_nodes.len(),
        diagnostics: parsed.diagnostics,
    })
}

pub(crate) async fn clear_snapshot(
    state: &AppState,
    profile_id: Uuid,
    source_id: &str,
) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM source_snapshots WHERE profile_id = $1 AND source_id = $2")
        .bind(profile_id)
        .bind(source_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

pub(crate) async fn load_snapshots(
    state: &Arc<AppState>,
    profile_id: Uuid,
    profile: &Profile,
) -> Result<Vec<SourceSnapshot>, ApiError> {
    let mut snapshots = Vec::new();
    for source in profile.sources.iter().filter(|source| source.enabled) {
        if source.kind == "raw" {
            validate_content(&source.content)?;
            snapshots.push(snapshot(&source.id, source.content.clone()));
            continue;
        }
        if source.kind != "url" || source.url.trim().is_empty() {
            return Err(ApiError::bad_request(format!(
                "source {:?} is invalid",
                source.id
            )));
        }
        let user_agent = if source.user_agent.trim().is_empty() {
            DEFAULT_USER_AGENT
        } else {
            source.user_agent.trim()
        };
        let cache_minutes = source
            .extra
            .get("cache_ttl_minutes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(60)
            .min(1440);
        if cache_minutes > 0
            && let Some(cached) = read_fresh_snapshot(
                state,
                profile_id,
                &source.id,
                i64::try_from(cache_minutes).unwrap_or(1440),
            )
            .await?
        {
            snapshots.push(cached);
            continue;
        }
        let proxy = (source
            .extra
            .get("fetch_mode")
            .and_then(serde_json::Value::as_str)
            == Some("domestic-direct"))
        .then_some(state.config.direct_proxy_url.as_deref())
        .flatten();
        match fetch_source_text(&source.url, user_agent, proxy).await {
            Ok(content) => {
                validate_content(&content)?;
                let value = snapshot(&source.id, content);
                store_snapshot(state, profile_id, &value).await?;
                snapshots.push(value);
            }
            Err(error) => {
                let fallback = read_snapshot(state, profile_id, &source.id).await?;
                let Some(fallback) = fallback else {
                    return Err(error);
                };
                tracing::warn!(profile_id = %profile_id, source_id = %source.id, error = ?error, "using last-known-good source snapshot");
                snapshots.push(fallback);
            }
        }
    }
    Ok(snapshots)
}

async fn fetch_source_text(
    input: &str,
    user_agent: &str,
    proxy: Option<&str>,
) -> Result<String, ApiError> {
    let mut last_error = None;
    for attempt in 1..=3 {
        match fetch_text(input, user_agent, MAX_SOURCE_SIZE, proxy).await {
            Ok(content) => return Ok(content),
            Err(error) => {
                last_error = Some(error);
                if attempt < 3 {
                    tokio::time::sleep(Duration::from_millis(200 * attempt)).await;
                }
            }
        }
    }
    Err(last_error.expect("three attempts always produce an error"))
}

pub(crate) async fn fetch_public_text(
    input: &str,
    user_agent: &str,
    max_size: usize,
) -> Result<String, ApiError> {
    fetch_text(input, user_agent, max_size, None).await
}

async fn fetch_text(
    input: &str,
    user_agent: &str,
    max_size: usize,
    proxy: Option<&str>,
) -> Result<String, ApiError> {
    let mut url: Url = input
        .parse()
        .map_err(|_| ApiError::bad_request("source URL is invalid"))?;
    for redirect in 0..=MAX_REDIRECTS {
        let client = safe_client(&url, proxy).await?;
        let response = client
            .get(url.clone())
            .header(header::USER_AGENT, user_agent)
            .send()
            .await
            .map_err(|error| ApiError::unavailable(format!("source request failed: {error}")))?;
        if response.status().is_redirection() {
            if redirect == MAX_REDIRECTS {
                return Err(ApiError::unavailable("source exceeded redirect limit"));
            }
            let location = response
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| ApiError::unavailable("source redirect has no valid location"))?;
            url = url
                .join(location)
                .map_err(|_| ApiError::unavailable("source redirect URL is invalid"))?;
            continue;
        }
        if response.status() != StatusCode::OK {
            return Err(ApiError::unavailable(format!(
                "source returned HTTP {}",
                response.status()
            )));
        }
        if response
            .content_length()
            .is_some_and(|length| length > max_size as u64)
        {
            return Err(ApiError::unavailable("upstream response is too large"));
        }
        let mut content = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| {
                ApiError::unavailable(format!("source response failed: {error}"))
            })?;
            if content.len().saturating_add(chunk.len()) > max_size {
                return Err(ApiError::unavailable("upstream response is too large"));
            }
            content.extend_from_slice(&chunk);
        }
        let content = String::from_utf8(content)
            .map_err(|_| ApiError::unavailable("source response is not UTF-8"))?;
        return Ok(content);
    }
    Err(ApiError::unavailable("source redirect failed"))
}

async fn safe_client(url: &Url, proxy: Option<&str>) -> Result<Client, ApiError> {
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ApiError::bad_request(
            "source URL must be HTTP(S) without credentials",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| ApiError::bad_request("source URL has no host"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| ApiError::bad_request("source URL has no port"))?;
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| ApiError::unavailable(format!("source DNS lookup failed: {error}")))?
        .collect();
    if addresses.is_empty() || addresses.iter().any(|address| !public_ip(address.ip())) {
        return Err(ApiError::forbidden(
            "source URL resolves to a non-public address",
        ));
    }
    let mut builder = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .resolve_to_addrs(host, &addresses);
    if let Some(proxy) = proxy {
        builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(ApiError::internal)?);
    }
    builder.build().map_err(ApiError::internal)
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_ipv4(ip),
        IpAddr::V6(ip) => public_ipv6(ip),
    }
}

fn public_ipv4(ip: Ipv4Addr) -> bool {
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.octets()[0] == 0
        || ip.octets()[0] >= 240
        || matches!(
            ip.octets(),
            [100, 64..=127, _, _] | [192, 0, 0, _] | [198, 18..=19, _, _]
        ))
}

fn public_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    !(ip.is_loopback()
        || ip.is_multicast()
        || ip.is_unspecified()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

fn validate_content(content: &str) -> Result<(), ApiError> {
    if content.is_empty() {
        return Err(ApiError::unavailable("source response is empty"));
    }
    if content.len() > MAX_SOURCE_SIZE {
        return Err(ApiError::unavailable("source response exceeds 32 MiB"));
    }
    let parsed = parse_subscription(content);
    if parsed.nodes.is_empty() {
        return Err(ApiError::unavailable(format!(
            "source has no usable nodes: {}",
            parsed.diagnostics.join("; ")
        )));
    }
    Ok(())
}

fn snapshot(source_id: &str, content: String) -> SourceSnapshot {
    let content_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
    SourceSnapshot {
        source_id: source_id.into(),
        content,
        content_hash,
    }
}

async fn store_snapshot(
    state: &AppState,
    profile_id: Uuid,
    snapshot: &SourceSnapshot,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO source_snapshots (profile_id, source_id, content, content_hash, fetched_at, last_status, last_error) VALUES ($1, $2, $3, $4, NOW(), 'ok', NULL) ON CONFLICT (profile_id, source_id) DO UPDATE SET content = EXCLUDED.content, content_hash = EXCLUDED.content_hash, fetched_at = EXCLUDED.fetched_at, last_status = 'ok', last_error = NULL")
        .bind(profile_id).bind(&snapshot.source_id).bind(&snapshot.content).bind(&snapshot.content_hash).execute(&state.pool).await?;
    Ok(())
}

async fn read_snapshot(
    state: &AppState,
    profile_id: Uuid,
    source_id: &str,
) -> Result<Option<SourceSnapshot>, ApiError> {
    let row = sqlx::query("SELECT content, content_hash FROM source_snapshots WHERE profile_id = $1 AND source_id = $2").bind(profile_id).bind(source_id).fetch_optional(&state.pool).await?;
    row.map(|row| {
        Ok(SourceSnapshot {
            source_id: source_id.into(),
            content: row.try_get("content").map_err(ApiError::internal)?,
            content_hash: row.try_get("content_hash").map_err(ApiError::internal)?,
        })
    })
    .transpose()
}

async fn read_fresh_snapshot(
    state: &AppState,
    profile_id: Uuid,
    source_id: &str,
    cache_minutes: i64,
) -> Result<Option<SourceSnapshot>, ApiError> {
    let row = sqlx::query("SELECT content, content_hash FROM source_snapshots WHERE profile_id = $1 AND source_id = $2 AND fetched_at >= NOW() - ($3 * INTERVAL '1 minute')")
        .bind(profile_id).bind(source_id).bind(cache_minutes).fetch_optional(&state.pool).await?;
    row.map(|row| {
        Ok(SourceSnapshot {
            source_id: source_id.into(),
            content: row.try_get("content").map_err(ApiError::internal)?,
            content_hash: row.try_get("content_hash").map_err(ApiError::internal)?,
        })
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::{public_ip, public_ipv4};
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn blocks_private_and_special_addresses() {
        for value in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.1.1",
            "100.64.0.1",
            "198.18.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
        ] {
            assert!(
                !public_ip(value.parse::<IpAddr>().expect("address")),
                "{value}"
            );
        }
        assert!(public_ipv4(Ipv4Addr::new(1, 1, 1, 1)));
    }
}
