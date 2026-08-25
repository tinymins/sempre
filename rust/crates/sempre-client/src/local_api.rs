use std::{path::Path, time::Duration};

use reqwest::{Client, Method, StatusCode};
use sempre_control::DaemonEndpoint;
use serde::de::DeserializeOwned;
use thiserror::Error;

const TOKEN_HEADER: &str = "x-sempre-daemon-token";

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error(transparent)]
    Control(#[from] sempre_control::ControlError),
    #[error("build local API client: {0}")]
    Build(#[source] reqwest::Error),
    #[error("call local API: {0}")]
    Http(#[source] reqwest::Error),
    #[error("local API returned HTTP {status}: {message}")]
    Status { status: StatusCode, message: String },
}

pub(crate) struct LocalApi {
    endpoint: DaemonEndpoint,
    client: Client,
}

impl LocalApi {
    pub(crate) fn discover(path: &Path) -> Result<Self, Error> {
        let endpoint = DaemonEndpoint::read(path)?;
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(Error::Build)?;
        Ok(Self { endpoint, client })
    }

    pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        self.request(Method::GET, path).await
    }

    pub(crate) async fn post<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        self.request(Method::POST, path).await
    }

    async fn request<T: DeserializeOwned>(&self, method: Method, path: &str) -> Result<T, Error> {
        let url = format!("{}{}", self.endpoint.base_url, path);
        let response = self
            .client
            .request(method, url)
            .header(TOKEN_HEADER, &self.endpoint.token)
            .send()
            .await
            .map_err(Error::Http)?;
        let status = response.status();
        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|error| error.to_string());
            return Err(Error::Status { status, message });
        }
        response.json().await.map_err(Error::Http)
    }
}
