use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

use futures_util::StreamExt as _;
use reqwest::{Client, StatusCode, header};
use url::Url;

use crate::error::ApiError;

const MAX_SOURCE_SIZE: usize = 32 << 20;
const MAX_REDIRECTS: usize = 5;

pub(crate) async fn fetch_source_text(
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
