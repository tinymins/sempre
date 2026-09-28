use std::{collections::BTreeMap, time::Duration};

use futures_util::StreamExt as _;
use reqwest::{Client, header};
use url::Url;

use crate::error::ApiError;

const MAX_SOURCE_SIZE: usize = 32 << 20;
const MAX_REDIRECTS: usize = 5;

pub(crate) struct FetchedText {
    pub content: String,
    pub status: u16,
    pub headers: BTreeMap<String, String>,
}

pub(crate) async fn fetch_source_text(
    input: &str,
    user_agent: &str,
    proxy: Option<&str>,
) -> Result<FetchedText, ApiError> {
    let mut last_error = None;
    let mut last_response = None;
    for attempt in 1..=3 {
        match fetch_text(input, user_agent, MAX_SOURCE_SIZE, proxy).await {
            Ok(content) if content.status == 200 => return Ok(content),
            Ok(content) => last_response = Some(content),
            Err(error) => {
                last_error = Some(error);
            }
        }
        if attempt < 3 {
            tokio::time::sleep(Duration::from_millis(200 * attempt)).await;
        }
    }
    last_response.ok_or_else(|| last_error.expect("three attempts always produce an error"))
}

async fn fetch_text(
    input: &str,
    user_agent: &str,
    max_size: usize,
    proxy: Option<&str>,
) -> Result<FetchedText, ApiError> {
    let mut url: Url = input
        .parse()
        .map_err(|_| ApiError::bad_request("source URL is invalid"))?;
    for redirect in 0..=MAX_REDIRECTS {
        let client = safe_client(&url, proxy)?;
        let response = client
            .get(url.clone())
            .header(header::USER_AGENT, user_agent)
            .send()
            .await
            .map_err(|error| {
                let reason = if error.is_timeout() {
                    "source request timed out"
                } else if error.is_connect() {
                    "source connection failed"
                } else {
                    "source transport failed"
                };
                ApiError::unavailable(reason)
            })?;
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
        if response
            .content_length()
            .is_some_and(|length| length > max_size as u64)
        {
            return Err(ApiError::unavailable("upstream response is too large"));
        }
        let headers = [
            header::CONTENT_TYPE,
            header::ETAG,
            header::LAST_MODIFIED,
            header::CACHE_CONTROL,
        ]
        .into_iter()
        .filter_map(|name| {
            response
                .headers()
                .get(&name)
                .and_then(|value| value.to_str().ok())
                .map(|value| (name.to_string(), value.chars().take(256).collect()))
        })
        .collect();
        let status = response.status().as_u16();
        let mut content = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| ApiError::unavailable("source response failed"))?;
            if content.len().saturating_add(chunk.len()) > max_size {
                return Err(ApiError::unavailable("upstream response is too large"));
            }
            content.extend_from_slice(&chunk);
        }
        let content = String::from_utf8(content)
            .map_err(|_| ApiError::unavailable("source response is not UTF-8"))?;
        return Ok(FetchedText {
            content,
            status,
            headers,
        });
    }
    Err(ApiError::unavailable("source redirect failed"))
}

fn safe_client(url: &Url, proxy: Option<&str>) -> Result<Client, ApiError> {
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ApiError::bad_request(
            "source URL must be HTTP(S) without credentials",
        ));
    }
    if url.host_str().is_none() {
        return Err(ApiError::bad_request("source URL has no host"));
    }
    let mut builder = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20));
    if let Some(proxy) = proxy {
        builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(ApiError::internal)?);
    }
    builder.build().map_err(ApiError::internal)
}
