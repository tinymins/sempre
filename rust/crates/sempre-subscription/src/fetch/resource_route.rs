use std::{collections::HashMap, sync::Arc, time::Duration};

use futures_util::StreamExt as _;
use reqwest::{Client, Proxy, Response, StatusCode, header, redirect::Policy};
use url::Url;

use super::{Fetcher, SourceProxyRoute};
use crate::SubscriptionError;

const MAX_REDIRECTS: usize = 5;
const TIMEOUT: Duration = Duration::from_mins(2);

/// Where the proxy rules send a request. `sempre-subscription` starts no processes, so a
/// proxy arrives already running, or as the reason it could not start.
pub enum ResourceRoute {
    System,
    Direct,
    Proxy(SourceProxyRoute),
    Unavailable(String),
}

#[derive(Clone)]
enum Compiled {
    System,
    Client(Client),
    Unavailable(Arc<str>),
}

/// Selects the outbound for each host, again on every redirect.
#[derive(Clone)]
pub struct ResourceRoutes {
    default: Compiled,
    hosts: Arc<HashMap<String, Compiled>>,
}

impl ResourceRoutes {
    pub fn new(
        default: ResourceRoute,
        hosts: Vec<(String, ResourceRoute)>,
    ) -> Result<Self, SubscriptionError> {
        Ok(Self {
            default: compile(default)?,
            hosts: Arc::new(
                hosts
                    .into_iter()
                    .map(|(host, route)| Ok((host, compile(route)?)))
                    .collect::<Result<_, SubscriptionError>>()?,
            ),
        })
    }

    /// Follows redirects and reads the body, failing on any status but success.
    pub async fn fetch(
        &self,
        url: &str,
        user_agent: &str,
        limit: usize,
        system: &Client,
    ) -> Result<Vec<u8>, SubscriptionError> {
        let response = self.follow(url, user_agent, system).await?;
        if !response.status().is_success() {
            return Err(fetch_error(format!(
                "server returned HTTP {}",
                response.status()
            )));
        }
        read_limited(response, limit).await
    }

    /// Reports the final status and body size without judging the status.
    pub async fn probe(
        &self,
        url: &str,
        user_agent: &str,
        limit: usize,
        system: &Client,
    ) -> Result<(u16, usize), SubscriptionError> {
        let response = self.follow(url, user_agent, system).await?;
        let status = response.status().as_u16();
        Ok((status, read_limited(response, limit).await?.len()))
    }

    async fn follow(
        &self,
        url: &str,
        user_agent: &str,
        system: &Client,
    ) -> Result<Response, SubscriptionError> {
        let mut url = Url::parse(url).map_err(|_| fetch_error(format!("invalid URL {url:?}")))?;
        for _ in 0..=MAX_REDIRECTS {
            let client = self.client_for(&url, system)?;
            let response = client
                .get(url.clone())
                .header(header::USER_AGENT, user_agent)
                .send()
                .await
                .map_err(|error| fetch_error(describe(&error)))?;
            if !is_redirect(response.status()) {
                return Ok(response);
            }
            let location = response
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| fetch_error("redirect has no location".into()))?;
            url = url
                .join(location)
                .map_err(|_| fetch_error("redirect has an invalid location".into()))?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err(fetch_error("refuse invalid redirect target".into()));
            }
        }
        Err(fetch_error("too many redirects".into()))
    }

    fn client_for<'a>(
        &'a self,
        url: &Url,
        system: &'a Client,
    ) -> Result<&'a Client, SubscriptionError> {
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        match self.hosts.get(&host).unwrap_or(&self.default) {
            Compiled::System => Ok(system),
            Compiled::Client(client) => Ok(client),
            Compiled::Unavailable(reason) => Err(fetch_error(format!(
                "the proxy for {host} is unavailable: {reason}"
            ))),
        }
    }
}

impl Fetcher {
    #[must_use]
    pub fn with_resource_routes(&self, routes: ResourceRoutes) -> Self {
        Self {
            resource_routes: Some(routes),
            ..self.clone()
        }
    }
}

fn is_redirect(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}

fn compile(route: ResourceRoute) -> Result<Compiled, SubscriptionError> {
    let builder = sempre_http::client_builder()
        .timeout(TIMEOUT)
        .redirect(Policy::none());
    let build = |result: reqwest::Result<Client>| {
        result
            .map(Compiled::Client)
            .map_err(|error| fetch_error(error.to_string()))
    };
    match route {
        ResourceRoute::System => Ok(Compiled::System),
        ResourceRoute::Unavailable(reason) => Ok(Compiled::Unavailable(reason.into())),
        ResourceRoute::Direct => build(builder.no_proxy().build()),
        ResourceRoute::Proxy(route) => {
            if !route.address.ip().is_loopback() {
                return Err(SubscriptionError::Invalid(
                    "resource proxy must be loopback".into(),
                ));
            }
            let proxy = Proxy::all(format!("http://{}", route.address))
                .map_err(|error| fetch_error(error.to_string()))?
                .basic_auth(&route.username, &route.password);
            build(builder.proxy(proxy).build())
        }
    }
}

async fn read_limited(response: Response, limit: usize) -> Result<Vec<u8>, SubscriptionError> {
    let mut content = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| fetch_error(describe(&error)))?;
        if content.len().saturating_add(chunk.len()) > limit {
            return Err(fetch_error(format!("response exceeds {limit} bytes")));
        }
        content.extend_from_slice(&chunk);
    }
    Ok(content)
}

fn describe(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(error) = source {
        message.push_str(": ");
        message.push_str(&error.to_string());
        source = error.source();
    }
    message
}

fn fetch_error(message: String) -> SubscriptionError {
    SubscriptionError::Fetch(message)
}

#[cfg(test)]
#[path = "resource_route_tests.rs"]
mod tests;
