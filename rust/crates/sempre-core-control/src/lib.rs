mod model;

use std::{fs, path::Path, time::Duration};

use futures_util::StreamExt as _;
use reqwest::{Client as HttpClient, Method, StatusCode, header};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use thiserror::Error;
use url::Url;

pub use model::{ConnectionSnapshot, Overview, Proxy, ProxyProvider, Rule};

const MAX_METADATA_SIZE: u64 = 64 << 10;
const MAX_RESPONSE_SIZE: usize = 16 << 20;
const MAX_ERROR_SIZE: usize = 4 << 10;

#[derive(Debug, Error)]
pub enum ControlError {
    #[error("no managed core control API is available")]
    Unavailable,
    #[error("managed core control metadata is invalid: {0}")]
    InvalidMetadata(String),
    #[error("{core} exposes {protocol} management, not a Clash-compatible REST API")]
    UnsupportedProtocol { core: String, protocol: String },
    #[error("read managed core control metadata: {0}")]
    Read(#[source] std::io::Error),
    #[error("decode managed core control metadata: {0}")]
    DecodeMetadata(#[source] serde_json::Error),
    #[error("call core API: {0}")]
    Http(#[source] reqwest::Error),
    #[error("core API returned HTTP {status}: {body}")]
    Status { status: u16, body: String },
    #[error("decode core API response: {0}")]
    Decode(#[source] serde_json::Error),
    #[error("core API response exceeds its {limit} byte limit")]
    ResponseTooLarge { limit: usize },
}

#[derive(Clone, Debug, Deserialize)]
struct Endpoint {
    core: String,
    protocol: String,
    base_url: String,
    secret: String,
}

#[derive(Clone)]
pub struct Client {
    core: String,
    base: Url,
    secret: String,
    http: HttpClient,
}

impl Client {
    pub fn from_file(path: &Path) -> Result<Self, ControlError> {
        let metadata = fs::metadata(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ControlError::Unavailable
            } else {
                ControlError::Read(error)
            }
        })?;
        if !metadata.is_file() || metadata.len() > MAX_METADATA_SIZE {
            return Err(ControlError::InvalidMetadata(
                "metadata is not a bounded regular file".into(),
            ));
        }
        let data = fs::read(path).map_err(ControlError::Read)?;
        let endpoint: Endpoint =
            serde_json::from_slice(&data).map_err(ControlError::DecodeMetadata)?;
        Self::new(endpoint)
    }

    fn new(endpoint: Endpoint) -> Result<Self, ControlError> {
        if endpoint.protocol != "clash-rest" {
            return Err(ControlError::UnsupportedProtocol {
                core: endpoint.core,
                protocol: endpoint.protocol,
            });
        }
        let base = Url::parse(&endpoint.base_url)
            .map_err(|_| ControlError::InvalidMetadata("base URL is invalid".into()))?;
        if endpoint.core.is_empty()
            || endpoint.secret.is_empty()
            || base.scheme() != "http"
            || !base
                .host_str()
                .is_some_and(|host| matches!(host, "127.0.0.1" | "::1" | "localhost"))
            || base.port().is_none()
            || base.path() != "/"
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err(ControlError::InvalidMetadata(
                "core, loopback base URL, or secret is invalid".into(),
            ));
        }
        let http = HttpClient::builder()
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(ControlError::Http)?;
        Ok(Self {
            core: endpoint.core,
            base,
            secret: endpoint.secret,
            http,
        })
    }

    pub async fn overview(&self) -> Result<Overview, ControlError> {
        let version: Version = self.get(&["version"], &[]).await?;
        let config = self.config().await.unwrap_or_default();
        let connections = self.connections().await.unwrap_or_default();
        Ok(Overview {
            core: self.core.clone(),
            version: version.version,
            mode: config
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            connections: connections.connections.len(),
            download: connections.download_total,
            upload: connections.upload_total,
        })
    }

    pub async fn config(&self) -> Result<Value, ControlError> {
        self.get(&["configs"], &[]).await
    }

    pub async fn patch_config(&self, patch: Value) -> Result<(), ControlError> {
        self.send(Method::PATCH, &["configs"], &[], Some(patch))
            .await
            .map(|_: Value| ())
    }

    pub async fn proxies(&self) -> Result<Vec<Proxy>, ControlError> {
        let response: ProxyResponse = self.get(&["proxies"], &[]).await?;
        Ok(model::proxy_map(response.proxies))
    }

    pub async fn select_proxy(&self, group: &str, proxy: &str) -> Result<(), ControlError> {
        self.send(
            Method::PUT,
            &["proxies", group],
            &[],
            Some(json!({ "name": proxy })),
        )
        .await
        .map(|_: Value| ())
    }

    pub async fn proxy_delay(
        &self,
        name: &str,
        test_url: &str,
        timeout: u64,
    ) -> Result<u64, ControlError> {
        let timeout = timeout.to_string();
        let response: Delay = self
            .get(
                &["proxies", name, "delay"],
                &[("url", test_url), ("timeout", &timeout)],
            )
            .await?;
        Ok(response.delay)
    }

    pub async fn providers(&self) -> Result<Vec<ProxyProvider>, ControlError> {
        let response: ProviderResponse = self.get(&["providers", "proxies"], &[]).await?;
        Ok(model::providers(response.providers))
    }

    pub async fn provider_action(&self, name: &str, healthcheck: bool) -> Result<(), ControlError> {
        let mut path = vec!["providers", "proxies", name];
        if healthcheck {
            path.push("healthcheck");
        }
        let method = if healthcheck {
            Method::GET
        } else {
            Method::PUT
        };
        self.send(method, &path, &[], None).await.map(|_: Value| ())
    }

    pub async fn rules(&self) -> Result<Vec<Rule>, ControlError> {
        let response: RuleResponse = self.get(&["rules"], &[]).await?;
        Ok(response.rules)
    }

    pub async fn rule_providers(&self) -> Result<Value, ControlError> {
        self.get(&["providers", "rules"], &[]).await
    }

    pub async fn update_rule_provider(&self, name: &str) -> Result<(), ControlError> {
        self.send(Method::PUT, &["providers", "rules", name], &[], None)
            .await
            .map(|_: Value| ())
    }

    pub async fn connections(&self) -> Result<ConnectionSnapshot, ControlError> {
        let response: model::RawConnections = self.get(&["connections"], &[]).await?;
        Ok(response.into())
    }

    pub async fn close_connection(&self, id: &str) -> Result<(), ControlError> {
        let path = if id.is_empty() {
            vec!["connections"]
        } else {
            vec!["connections", id]
        };
        self.send(Method::DELETE, &path, &[], None)
            .await
            .map(|_: Value| ())
    }

    pub async fn dns_query(&self, name: &str, record_type: &str) -> Result<Value, ControlError> {
        self.get(&["dns", "query"], &[("name", name), ("type", record_type)])
            .await
    }

    pub async fn flush_fake_ip(&self) -> Result<(), ControlError> {
        self.send(Method::POST, &["cache", "fakeip", "flush"], &[], None)
            .await
            .map(|_: Value| ())
    }

    async fn get<T: DeserializeOwned>(
        &self,
        path: &[&str],
        query: &[(&str, &str)],
    ) -> Result<T, ControlError> {
        self.send(Method::GET, path, query, None).await
    }

    async fn send<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &[&str],
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<T, ControlError> {
        let mut endpoint = self.base.clone();
        endpoint
            .path_segments_mut()
            .map_err(|()| ControlError::InvalidMetadata("base URL cannot join paths".into()))?
            .pop_if_empty()
            .extend(path);
        endpoint
            .query_pairs_mut()
            .extend_pairs(query.iter().copied());
        let mut request = self
            .http
            .request(method, endpoint)
            .header(header::ACCEPT, "application/json")
            .bearer_auth(&self.secret);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(ControlError::Http)?;
        let status = response.status();
        let limit = if status.is_success() {
            MAX_RESPONSE_SIZE
        } else {
            MAX_ERROR_SIZE
        };
        let body = limited_body(response, limit).await?;
        if !status.is_success() {
            return Err(ControlError::Status {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).trim().into(),
            });
        }
        if status == StatusCode::NO_CONTENT || body.is_empty() {
            return serde_json::from_value(Value::Null).map_err(ControlError::Decode);
        }
        serde_json::from_slice(&body).map_err(ControlError::Decode)
    }
}

async fn limited_body(response: reqwest::Response, limit: usize) -> Result<Vec<u8>, ControlError> {
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(ControlError::Http)?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(ControlError::ResponseTooLarge { limit });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[derive(Deserialize)]
struct Version {
    version: String,
}

#[derive(Deserialize)]
struct Delay {
    delay: u64,
}

#[derive(Deserialize)]
struct ProxyResponse {
    proxies: serde_json::Map<String, Value>,
}

#[derive(Deserialize)]
struct ProviderResponse {
    providers: serde_json::Map<String, Value>,
}

#[derive(Deserialize)]
struct RuleResponse {
    #[serde(default)]
    rules: Vec<Rule>,
}

#[cfg(test)]
mod tests {
    use axum::{Json, Router, http::HeaderMap, routing::get};

    use super::*;

    #[test]
    fn metadata_requires_loopback_clash_rest_with_a_secret() {
        let valid = Endpoint {
            core: "mihomo".into(),
            protocol: "clash-rest".into(),
            base_url: "http://127.0.0.1:9090".into(),
            secret: "secret".into(),
        };
        assert!(Client::new(valid.clone()).is_ok());
        for endpoint in [
            Endpoint {
                base_url: "http://192.0.2.1:9090".into(),
                ..valid.clone()
            },
            Endpoint {
                protocol: "grpc".into(),
                ..valid.clone()
            },
            Endpoint {
                secret: String::new(),
                ..valid
            },
        ] {
            assert!(Client::new(endpoint).is_err());
        }
    }

    fn authorized(headers: &HeaderMap) {
        assert_eq!(
            headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer secret")
        );
    }

    #[tokio::test]
    async fn client_aggregates_and_normalizes_a_loopback_clash_api() {
        let app = Router::new()
            .route(
                "/version",
                get(|headers: HeaderMap| async move {
                    authorized(&headers);
                    Json(json!({ "version": "1.2.3" }))
                }),
            )
            .route(
                "/configs",
                get(|| async { Json(json!({ "mode": "rule" })) }),
            )
            .route(
                "/connections",
                get(|| async {
                    Json(json!({
                        "downloadTotal": 10, "uploadTotal": 20,
                        "connections": [{ "id": "one", "metadata": {}, "chains": [] }]
                    }))
                }),
            )
            .route(
                "/proxies",
                get(|| async {
                    Json(json!({
                        "proxies": { "Fallback": { "type": "Selector", "now": "edge" } }
                    }))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("fake core server");
        });
        let client = Client::new(Endpoint {
            core: "mihomo".into(),
            protocol: "clash-rest".into(),
            base_url: format!("http://{address}"),
            secret: "secret".into(),
        })
        .expect("client");
        let overview = client.overview().await.expect("overview");
        assert_eq!(overview.version, "1.2.3");
        assert_eq!(overview.mode, "rule");
        assert_eq!(overview.connections, 1);
        assert_eq!(overview.download, 10);
        let proxies = client.proxies().await.expect("proxies");
        assert_eq!(proxies[0].name, "Fallback");
        assert_eq!(proxies[0].proxy_type, "Selector");
        server.abort();
    }
}
