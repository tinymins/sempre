use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::StreamExt as _;
use reqwest::{Client, redirect::Policy};
use sempre_converter::{Profile, Target};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use url::Url;

use crate::{MAX_SOURCE_SIZE, SubscriptionError};

const MAX_MANIFEST_SIZE: usize = 1 << 20;

#[derive(Clone, Debug)]
pub struct RemoteResult {
    pub content: String,
    pub profile: Profile,
    pub target: Target,
    pub artifact_hash: String,
    pub node_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    schema: u32,
    service: String,
    profile: ManifestProfile,
    target: Target,
    artifact: ManifestArtifact,
    runtime: Option<ManifestRuntime>,
    edit_url: String,
    read_only: bool,
}

#[derive(Debug, Deserialize)]
struct ManifestProfile {
    name: String,
    revision: i64,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct ManifestArtifact {
    url: String,
    sha256: String,
    node_count: i64,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct ManifestRuntime {
    local_proxy: Value,
    transparent_proxy: Value,
    management_api: Value,
}

pub struct RemoteClient {
    http: Client,
}

impl RemoteClient {
    pub fn new() -> Result<Self, SubscriptionError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(Policy::none())
            .build()
            .map_err(|error| SubscriptionError::Fetch(error.to_string()))?;
        Ok(Self { http })
    }

    pub async fn render(
        &self,
        profile: &Profile,
        target: &Target,
    ) -> Result<RemoteResult, SubscriptionError> {
        let remote = profile
            .extra
            .get("remote")
            .and_then(Value::as_object)
            .ok_or_else(|| SubscriptionError::Fetch("remote profile has no settings".into()))?;
        let manifest_value = remote
            .get("manifest_url")
            .and_then(Value::as_str)
            .ok_or_else(|| SubscriptionError::Fetch("remote profile has no manifest URL".into()))?;
        let mut manifest_url = parse_url(manifest_value)?;
        manifest_url
            .query_pairs_mut()
            .append_pair("target", &target.format);
        let manifest_data = self.get(&manifest_url, MAX_MANIFEST_SIZE).await?;
        let manifest: Manifest = serde_json::from_slice(&manifest_data)
            .map_err(|error| SubscriptionError::Fetch(format!("decode manifest: {error}")))?;
        validate_manifest(&manifest, &target.format)?;
        let artifact_url = manifest_url
            .join(&manifest.artifact.url)
            .map_err(|_| SubscriptionError::Fetch("remote artifact URL is invalid".into()))?;
        if !same_origin(&manifest_url, &artifact_url) {
            return Err(SubscriptionError::Fetch(
                "remote artifact URL must use the manifest origin".into(),
            ));
        }
        let artifact = self.get(&artifact_url, MAX_SOURCE_SIZE).await?;
        let content = String::from_utf8(artifact)
            .map_err(|_| SubscriptionError::Fetch("remote artifact is not UTF-8".into()))?;
        let actual_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
        if !actual_hash.eq_ignore_ascii_case(&manifest.artifact.sha256) {
            return Err(SubscriptionError::Fetch(
                "remote artifact SHA-256 does not match its manifest".into(),
            ));
        }
        let mut updated = profile.clone();
        if let Some(runtime) = &manifest.runtime {
            updated.local_proxy =
                serde_json::from_value(runtime.local_proxy.clone()).map_err(|error| {
                    SubscriptionError::Fetch(format!("invalid local proxy: {error}"))
                })?;
            updated.transparent_proxy = serde_json::from_value(runtime.transparent_proxy.clone())
                .map_err(|error| {
                SubscriptionError::Fetch(format!("invalid transparent proxy: {error}"))
            })?;
            updated.management_api = serde_json::from_value(runtime.management_api.clone())
                .map_err(|error| {
                    SubscriptionError::Fetch(format!("invalid management API: {error}"))
                })?;
        }
        updated.extra.insert(
            "remote".into(),
            Value::Object(remote_metadata(remote, &manifest, &actual_hash)),
        );
        Ok(RemoteResult {
            content,
            profile: updated,
            target: manifest.target,
            artifact_hash: actual_hash,
            node_count: usize::try_from(manifest.artifact.node_count)
                .map_err(|_| SubscriptionError::Fetch("invalid remote node count".into()))?,
            warnings: vec!["configuration supplied by the remote Sempre conversion service".into()],
        })
    }

    async fn get(&self, url: &Url, limit: usize) -> Result<Vec<u8>, SubscriptionError> {
        let response = self
            .http
            .get(url.clone())
            .header(
                reqwest::header::ACCEPT,
                "application/json, application/yaml, text/plain",
            )
            .header(
                reqwest::header::USER_AGENT,
                "sempre-client/remote-subscription",
            )
            .send()
            .await
            .map_err(|error| SubscriptionError::Fetch(error.to_string()))?;
        if !response.status().is_success() {
            return Err(SubscriptionError::Fetch(format!(
                "server returned HTTP {}",
                response.status()
            )));
        }
        let mut content = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| SubscriptionError::Fetch(error.to_string()))?;
            if content.len().saturating_add(chunk.len()) > limit {
                return Err(SubscriptionError::Fetch(format!(
                    "response exceeds {limit} bytes"
                )));
            }
            content.extend_from_slice(&chunk);
        }
        Ok(content)
    }
}

fn validate_manifest(manifest: &Manifest, expected_target: &str) -> Result<(), SubscriptionError> {
    if manifest.schema != 1 || manifest.service != "sempre" || !manifest.read_only {
        return Err(SubscriptionError::Fetch(
            "unsupported remote subscription manifest".into(),
        ));
    }
    if manifest.target.format != expected_target {
        return Err(SubscriptionError::Fetch(format!(
            "remote target {:?} does not match {expected_target:?}",
            manifest.target.format
        )));
    }
    if manifest.profile.revision < 1
        || manifest.profile.name.trim().is_empty()
        || manifest.artifact.node_count < 0
        || manifest.artifact.sha256.len() != 64
        || !manifest
            .artifact
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(SubscriptionError::Fetch(
            "remote subscription manifest is incomplete".into(),
        ));
    }
    if !manifest.edit_url.is_empty() {
        parse_url(&manifest.edit_url)?;
    }
    Ok(())
}

fn remote_metadata(
    previous: &Map<String, Value>,
    manifest: &Manifest,
    hash: &str,
) -> Map<String, Value> {
    let mut remote = previous.clone();
    remote.insert("edit_url".into(), json!(manifest.edit_url));
    remote.insert("server_profile".into(), json!(manifest.profile.name));
    remote.insert("server_revision".into(), json!(manifest.profile.revision));
    remote.insert("artifact_sha256".into(), json!(hash));
    remote.insert("target".into(), json!(manifest.target.format));
    remote.insert("node_count".into(), json!(manifest.artifact.node_count));
    remote.insert(
        "server_updated_at".into(),
        json!(manifest.profile.updated_at),
    );
    remote.insert(
        "artifact_created_at".into(),
        json!(manifest.artifact.created_at),
    );
    remote.insert("last_synced_at".into(), json!(Utc::now()));
    remote
}

fn parse_url(value: &str) -> Result<Url, SubscriptionError> {
    let parsed = Url::parse(value.trim())
        .map_err(|_| SubscriptionError::Fetch("remote subscription URL is invalid".into()))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(SubscriptionError::Fetch(
            "remote subscription URL must be HTTP(S) without credentials".into(),
        ));
    }
    Ok(parsed)
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host() == right.host()
        && left.port_or_known_default() == right.port_or_known_default()
        && right.username().is_empty()
        && right.password().is_none()
}

#[cfg(test)]
mod tests;
