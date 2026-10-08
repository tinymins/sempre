use std::time::Duration;

use sempre_control::{API_MAJOR, PublicEndpoint, local_url};
use sempre_state::Layout;
use serde::Deserialize;
use tokio::time::{Instant, sleep};

use crate::ManagerError;

#[derive(Deserialize)]
struct Health {
    status: String,
    version: String,
    api_major: u32,
}

pub(super) async fn wait_for_health(
    layout: &Layout,
    expected_version: &str,
    timeout: Duration,
) -> Result<(), ManagerError> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| ManagerError::RuntimeNotReady(error.to_string()))?;
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let result = probe(layout, expected_version, &client, remaining).await;
        if result.is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return result;
        }
        sleep(Duration::from_millis(250).min(deadline.saturating_duration_since(Instant::now())))
            .await;
    }
}

async fn probe(
    layout: &Layout,
    expected_version: &str,
    client: &reqwest::Client,
    remaining: Duration,
) -> Result<(), ManagerError> {
    let endpoint = PublicEndpoint::read(&layout.endpoint)
        .map_err(|error| ManagerError::RuntimeNotReady(error.to_string()))?;
    if endpoint.version != expected_version {
        return Err(ManagerError::RuntimeNotReady(format!(
            "daemon reports version {}, expected {expected_version}",
            endpoint.version
        )));
    }
    let url = format!("{}/api/v1/health", local_url(&endpoint.bind)?);
    let health = client
        .get(url)
        .timeout(remaining.min(Duration::from_secs(2)))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| ManagerError::RuntimeNotReady(error.to_string()))?
        .json::<Health>()
        .await
        .map_err(|error| ManagerError::RuntimeNotReady(error.to_string()))?;
    if health.status != "ok" || health.version != expected_version || health.api_major != API_MAJOR
    {
        return Err(ManagerError::RuntimeNotReady(format!(
            "health response is not ready for Sempre {expected_version}, API {API_MAJOR}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
