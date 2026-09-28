use std::{collections::BTreeMap, time::Instant};

use futures_util::StreamExt as _;
use reqwest::Client;
use serde::Serialize;

use crate::{MAX_SOURCE_SIZE, SubscriptionError};

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchObservation {
    pub http_status: Option<u16>,
    pub http_headers: Option<BTreeMap<String, String>>,
    pub fetch_duration_ms: Option<u128>,
    pub final_url: Option<String>,
    pub attempts: usize,
    pub body_preview: Option<String>,
    pub body_bytes: Option<usize>,
    pub body_truncated: bool,
}

pub(super) async fn download_once(
    client: &Client,
    url: &str,
    user_agent: &str,
) -> Result<(Vec<u8>, FetchObservation), (SubscriptionError, FetchObservation)> {
    let started = Instant::now();
    let mut observation = FetchObservation {
        attempts: 1,
        ..FetchObservation::default()
    };
    let response = match client
        .get(url)
        .header(reqwest::header::USER_AGENT, user_agent)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            observation.fetch_duration_ms = Some(started.elapsed().as_millis());
            return Err((SubscriptionError::Fetch(error.to_string()), observation));
        }
    };
    observation.http_status = Some(response.status().as_u16());
    observation.final_url = Some(response.url().to_string());
    let mut headers = BTreeMap::new();
    for name in [
        reqwest::header::CONTENT_TYPE,
        reqwest::header::CONTENT_LENGTH,
        reqwest::header::CACHE_CONTROL,
        reqwest::header::ETAG,
        reqwest::header::LAST_MODIFIED,
    ] {
        if let Some(value) = response.headers().get(&name)
            && let Ok(value) = value.to_str()
        {
            headers.insert(name.as_str().into(), value.into());
        }
    }
    observation.http_headers = Some(headers);
    if response.status() != reqwest::StatusCode::OK {
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let Ok(chunk) = chunk else { break };
            let remaining = 16_384_usize.saturating_sub(body.len());
            body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
            if chunk.len() > remaining {
                observation.body_truncated = true;
                break;
            }
        }
        observation.body_bytes = Some(body.len());
        observation.body_preview = Some(String::from_utf8_lossy(&body).into_owned());
        observation.fetch_duration_ms = Some(started.elapsed().as_millis());
        return Err((
            SubscriptionError::Fetch(format!(
                "HTTP {}",
                observation.http_status.unwrap_or_default()
            )),
            observation,
        ));
    }
    let mut content = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => {
                observation.fetch_duration_ms = Some(started.elapsed().as_millis());
                return Err((SubscriptionError::Fetch(error.to_string()), observation));
            }
        };
        if content.len().saturating_add(chunk.len()) > MAX_SOURCE_SIZE {
            observation.fetch_duration_ms = Some(started.elapsed().as_millis());
            return Err((
                SubscriptionError::SourceTooLarge {
                    limit: MAX_SOURCE_SIZE,
                },
                observation,
            ));
        }
        content.extend_from_slice(&chunk);
    }
    observation.fetch_duration_ms = Some(started.elapsed().as_millis());
    if content.iter().all(u8::is_ascii_whitespace) {
        return Err((
            SubscriptionError::Fetch("response is empty".into()),
            observation,
        ));
    }
    Ok((content, observation))
}
