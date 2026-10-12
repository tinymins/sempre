//! Server-owned resource downloads, retaining binary bodies and upstream statuses.
use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderName, HeaderValue, StatusCode},
    response::Response,
    routing::get,
};
use serde::Deserialize;

use crate::{
    AppState,
    error::ApiError,
    fetch::{self, FetchedBytes},
    relay_egress::{self, Outbound, ResolvedRoute},
};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/proxy/fetch", get(download))
}

#[derive(Deserialize)]
struct ResourceQuery {
    url: String,
}

pub(crate) struct RelayFetch {
    pub response: FetchedBytes,
    pub route: ResolvedRoute,
}

/// No body cache: admin probes always exercise the saved outbound policy.
pub(crate) async fn fetch(state: &AppState, input: &str) -> Result<RelayFetch, ApiError> {
    let policy = relay_egress::Policy::load(state).await?;
    fetch_with_policy(state, input, &policy).await
}

pub(crate) async fn fetch_with_policy(
    state: &AppState,
    input: &str,
    policy: &relay_egress::Policy,
) -> Result<RelayFetch, ApiError> {
    let last_url = std::sync::Mutex::new(None);
    fetch_with_policy_observed(state, input, policy, &last_url).await
}

pub(crate) async fn fetch_with_policy_observed(
    state: &AppState,
    input: &str,
    policy: &relay_egress::Policy,
    last_url: &std::sync::Mutex<Option<url::Url>>,
) -> Result<RelayFetch, ApiError> {
    relay_egress::validate_url(input)?;
    let (response, route) = fetch::fetch_bytes(
        input,
        "sempre-resource-relay/1",
        fetch::MAX_SOURCE_SIZE,
        |url| async move {
            *last_url.lock().map_err(ApiError::internal)? = Some(url.clone());
            if url.fragment().is_some() {
                return Err(ApiError::bad_request(
                    "resource URL cannot contain a fragment",
                ));
            }
            let own_relay = state
                .config
                .public_url
                .join("api/proxy/")
                .map_err(ApiError::internal)?;
            if url.origin() == own_relay.origin() && url.path().starts_with(own_relay.path()) {
                return Err(ApiError::bad_request(
                    "relay URL cannot target another relay endpoint",
                ));
            }
            let addresses = public_addresses(&url).await?;
            let route = policy.resolve(state, &url).await?;
            let mut client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(20))
                .resolve_to_addrs(url.host_str().expect("validated host"), &addresses);
            match &route.outbound {
                Outbound::System => {}
                Outbound::Direct => client = client.no_proxy(),
                Outbound::Proxy { .. } => {
                    let proxy = route
                        .proxy_url
                        .as_deref()
                        .ok_or_else(|| ApiError::unavailable("relay proxy is unavailable"))?;
                    client = client
                        .no_proxy()
                        .proxy(reqwest::Proxy::all(proxy).map_err(ApiError::internal)?);
                }
            }
            Ok((client.build().map_err(ApiError::internal)?, route))
        },
    )
    .await?;
    Ok(RelayFetch { response, route })
}

async fn download(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ResourceQuery>,
) -> Result<Response, ApiError> {
    let fetched = fetch(&state, &query.url).await?.response;
    let mut response = Response::new(Body::from(fetched.content));
    *response.status_mut() = StatusCode::from_u16(fetched.status).map_err(ApiError::internal)?;
    for (name, value) in fetched.headers {
        if let (Ok(name), Ok(value)) = (HeaderName::try_from(name), HeaderValue::try_from(value)) {
            response.headers_mut().insert(name, value);
        }
    }
    // Arbitrary remote HTML must not execute with the control plane's origin.
    response.headers_mut().insert(
        "content-security-policy",
        HeaderValue::from_static("sandbox; default-src 'none'"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

async fn public_addresses(url: &url::Url) -> Result<Vec<std::net::SocketAddr>, ApiError> {
    let host = url
        .host_str()
        .ok_or_else(|| ApiError::bad_request("resource URL requires a host"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| ApiError::bad_request("resource URL requires a port"))?;
    let addresses = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::net::lookup_host((host.trim_matches(['[', ']']), port)),
    )
    .await
    .map_err(|_| ApiError::unavailable("resource DNS lookup timed out"))?
    .map_err(|_| ApiError::unavailable("resource DNS lookup failed"))?
    .collect::<Vec<_>>();
    if addresses.is_empty() || addresses.iter().any(|address| !public_ip(address.ip())) {
        return Err(ApiError::bad_request(
            "resource URL must resolve only to public addresses",
        ));
    }
    Ok(addresses)
}

fn public_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192
                    && (b == 168 || (b == 0 && (c == 0 || c == 2)) || (b == 88 && c == 99)))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        std::net::IpAddr::V6(ip) => {
            let segments = ip.segments();
            // Global unicast only; exclude IETF special assignments and documentation.
            (segments[0] & 0xe000) == 0x2000
                && !(segments[0] == 0x2001 && (segments[1] < 0x200 || segments[1] == 0xdb8))
                && segments[0] != 0x2002
                && !(segments[0] == 0x3fff && segments[1] < 0x1000)
        }
    }
}
