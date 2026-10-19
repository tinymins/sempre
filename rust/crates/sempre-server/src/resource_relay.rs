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

pub(crate) struct FetchOptions<'a> {
    pub user_agent: &'a str,
    pub max_size: usize,
    pub timeout: Duration,
}

impl Default for FetchOptions<'_> {
    fn default() -> Self {
        Self {
            user_agent: "sempre-resource-relay/1",
            max_size: fetch::MAX_SOURCE_SIZE,
            timeout: Duration::from_secs(20),
        }
    }
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
    fetch_with_policy_options(state, input, policy, FetchOptions::default()).await
}

pub(crate) async fn fetch_with_policy_options(
    state: &AppState,
    input: &str,
    policy: &relay_egress::Policy,
    options: FetchOptions<'_>,
) -> Result<RelayFetch, ApiError> {
    let last_url = std::sync::Mutex::new(None);
    fetch_with_options_observed(state, input, policy, options, &last_url).await
}

pub(crate) async fn fetch_with_policy_observed(
    state: &AppState,
    input: &str,
    policy: &relay_egress::Policy,
    last_url: &std::sync::Mutex<Option<url::Url>>,
) -> Result<RelayFetch, ApiError> {
    fetch_with_options_observed(state, input, policy, FetchOptions::default(), last_url).await
}

async fn fetch_with_options_observed(
    state: &AppState,
    input: &str,
    policy: &relay_egress::Policy,
    options: FetchOptions<'_>,
    last_url: &std::sync::Mutex<Option<url::Url>>,
) -> Result<RelayFetch, ApiError> {
    relay_egress::validate_url(input)?;
    let (response, route) = fetch::fetch_bytes(
        input,
        options.user_agent,
        options.max_size,
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
            let route = policy.resolve(state, &url).await?;
            let mut client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(10))
                .timeout(options.timeout);
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
