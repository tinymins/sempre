//! Reads and validates the published release manifest.
use std::{collections::HashSet, time::Duration};

use reqwest::{Client, ClientBuilder, StatusCode};
use url::Url;

use super::{
    MAX_MANIFEST_SIZE, Manifest, PREVIEW_MANIFEST_URL, STABLE_MANIFEST_URL, VERSION,
    describe_error, parse_version,
};

fn manifest_client_builder() -> ClientBuilder {
    sempre_http::client_builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("too many redirects")
            } else if attempt.url().scheme() != "https" {
                attempt.error("refuse non-HTTPS redirect")
            } else {
                attempt.follow()
            }
        }))
        .user_agent(format!("Sempre/{VERSION}"))
}

pub(super) async fn fetch_manifest(
    manager: &sempre_manager::Manager,
    allow_prerelease: bool,
) -> Result<Manifest, String> {
    let url = if allow_prerelease {
        PREVIEW_MANIFEST_URL
    } else {
        STABLE_MANIFEST_URL
    };
    let lease = manager
        .resource_egress(url)
        .await
        .map_err(|error| error.to_string())?;
    if !lease.is_system() {
        let client = lease
            .egress
            .apply(manifest_client_builder())
            .and_then(ClientBuilder::build)
            .map_err(|error| format!("build update client: {error}"))?;
        let body = fetch_manifest_body(&client, url).await?;
        return parse_manifest(&body, allow_prerelease);
    }
    let proxy = crate::service_update_proxy::client(manager, manifest_client_builder())
        .map(|client| async move { fetch_manifest_body(&client, url).await });
    let body = crate::service_update_proxy::proxy_first(
        "query Sempre update service",
        proxy,
        |_| Ok(()),
        || async {
            let client = manifest_client_builder()
                .build()
                .map_err(|error| format!("build direct update client: {error}"))?;
            fetch_manifest_body(&client, url).await
        },
    )
    .await?;
    parse_manifest(&body, allow_prerelease)
}

fn parse_manifest(body: &[u8], allow_prerelease: bool) -> Result<Manifest, String> {
    let manifest: Manifest = serde_json::from_slice(body)
        .map_err(|error| format!("decode Sempre update manifest: {error}"))?;
    validate_manifest(&manifest, allow_prerelease)?;
    Ok(manifest)
}

async fn fetch_manifest_body(client: &Client, url: &str) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| describe_error(&error))?;
    if response.status() != StatusCode::OK {
        return Err(format!(
            "query Sempre update service: HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MANIFEST_SIZE as u64)
    {
        return Err("Sempre update manifest exceeds 1 MiB".into());
    }
    let body = response
        .bytes()
        .await
        .map_err(|error| format!("read Sempre update manifest: {error}"))?;
    if body.len() > MAX_MANIFEST_SIZE {
        return Err("Sempre update manifest exceeds 1 MiB".into());
    }
    Ok(body.to_vec())
}

pub(super) fn validate_manifest(manifest: &Manifest, allow_prerelease: bool) -> Result<(), String> {
    if manifest.schema != 1 {
        return Err(format!(
            "unsupported Sempre update manifest schema {}",
            manifest.schema
        ));
    }
    let latest = parse_version(&manifest.version)?;
    if !allow_prerelease && !latest.pre.is_empty() {
        return Err("update manifest latest version is a prerelease".into());
    }
    let repository = Url::parse(&manifest.repository)
        .map_err(|_| "update manifest has an invalid repository URL".to_string())?;
    if repository.scheme() != "https" || repository.host_str().is_none() {
        return Err("update manifest has an invalid repository URL".into());
    }
    if manifest.published_at.len() > 128 || manifest.notes.len() > 256 * 1024 {
        return Err("update manifest metadata is too large".into());
    }
    let mut versions = HashSet::new();
    for release in &manifest.releases {
        let version = parse_version(&release.version)?;
        if !allow_prerelease && !version.pre.is_empty() {
            return Err("update manifest history contains a prerelease".into());
        }
        if !versions.insert(version) {
            return Err("update manifest history contains a duplicate version".into());
        }
        if release.published_at.len() > 128 || release.notes.len() > 256 * 1024 {
            return Err("update manifest release metadata is too large".into());
        }
    }
    Ok(())
}
