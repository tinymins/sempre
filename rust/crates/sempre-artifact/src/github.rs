use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures_util::StreamExt;
use reqwest::{Client, StatusCode, header};
use sempre_core::{Adapter, Package, STABLE, Target};
use serde::Deserialize;
use url::Url;

use crate::{ArtifactError, Result, Sha256Digest, https_redirect_policy};

const MAX_RELEASE_RESPONSE: usize = 4 << 20;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ReleaseAsset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
    #[serde(default)]
    pub digest: String,
    pub size: u64,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Release {
    #[serde(rename = "tag_name")]
    pub tag: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Clone)]
pub struct GithubClient {
    client: Client,
    base: Url,
    token: Option<String>,
    cache: Arc<Mutex<HashMap<String, Release>>>,
}

impl GithubClient {
    pub fn new(user_agent: &str) -> Result<Self> {
        Self::with_base(
            user_agent,
            Url::parse("https://api.github.com").expect("valid URL"),
        )
    }

    fn with_base(user_agent: &str, base: Url) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(https_redirect_policy())
            .user_agent(user_agent)
            .build()
            .map_err(|error| ArtifactError::http("build GitHub client", error))?;
        let token = std::env::var("GITHUB_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| std::env::var("GH_TOKEN").ok())
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        Ok(Self {
            client,
            base,
            token,
            cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn resolve(
        &self,
        adapter: &dyn Adapter,
        repository: &str,
        reference: &str,
        target: &Target,
    ) -> Result<Package> {
        let repository = if repository.is_empty() {
            adapter.default_repository()
        } else {
            repository
        };
        let release = self.release(repository, reference).await?;
        if reference == STABLE && release.prerelease {
            return Err(ArtifactError::invalid(format!(
                "latest release {} is a prerelease",
                release.tag
            )));
        }
        package_from_release(adapter, &release, target)
    }

    async fn release(&self, repository: &str, reference: &str) -> Result<Release> {
        let endpoint = release_endpoint(&self.base, repository, reference)?;
        if let Some(release) = self
            .cache
            .lock()
            .expect("release cache lock")
            .get(endpoint.as_str())
            .cloned()
        {
            return Ok(release);
        }
        let mut request = self
            .client
            .get(endpoint.clone())
            .header(header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .map_err(|error| ArtifactError::http("query GitHub release", error))?;
        if response.status() != StatusCode::OK {
            return Err(ArtifactError::invalid(format!(
                "query GitHub release: HTTP {}",
                response.status()
            )));
        }
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| ArtifactError::http("read GitHub release", error))?;
            if body.len().saturating_add(chunk.len()) > MAX_RELEASE_RESPONSE {
                return Err(ArtifactError::invalid(
                    "GitHub release response exceeds 4 MiB",
                ));
            }
            body.extend_from_slice(&chunk);
        }
        let release: Release = serde_json::from_slice(&body)
            .map_err(|error| ArtifactError::invalid(format!("decode GitHub release: {error}")))?;
        if release.draft {
            return Err(ArtifactError::invalid(format!(
                "GitHub release {} is a draft",
                release.tag
            )));
        }
        self.cache
            .lock()
            .expect("release cache lock")
            .insert(endpoint.into(), release.clone());
        Ok(release)
    }
}

fn release_endpoint(base: &Url, repository: &str, reference: &str) -> Result<Url> {
    let mut repository_parts = repository.split('/');
    let owner = repository_parts.next().unwrap_or_default();
    let name = repository_parts.next().unwrap_or_default();
    if owner.is_empty()
        || name.is_empty()
        || repository_parts.next().is_some()
        || [owner, name]
            .iter()
            .any(|part| *part == "." || *part == "..")
    {
        return Err(ArtifactError::invalid(format!(
            "invalid GitHub repository {repository:?}"
        )));
    }
    let mut endpoint = base.clone();
    {
        let mut segments = endpoint
            .path_segments_mut()
            .map_err(|()| ArtifactError::invalid("GitHub base URL cannot contain path segments"))?;
        segments
            .pop_if_empty()
            .extend(["repos", owner, name, "releases"]);
        if reference == STABLE {
            segments.push("latest");
        } else {
            segments.extend([
                "tags",
                &format!("v{}", reference.strip_prefix('v').unwrap_or(reference)),
            ]);
        }
    }
    Ok(endpoint)
}

fn package_from_release(
    adapter: &dyn Adapter,
    release: &Release,
    target: &Target,
) -> Result<Package> {
    let version = release.tag.strip_prefix('v').unwrap_or(&release.tag);
    let selection = adapter
        .package_assets(version, target)
        .map_err(|error| ArtifactError::invalid(error.to_string()))?;
    for name in &selection.names {
        if let Some(asset) = release.assets.iter().find(|asset| asset.name == *name) {
            asset.digest.parse::<Sha256Digest>().map_err(|_| {
                ArtifactError::invalid(format!(
                    "release asset {name} does not provide a valid SHA-256 digest"
                ))
            })?;
            return Ok(Package {
                version: version.into(),
                name: name.clone(),
                url: asset.url.clone(),
                digest: asset.digest.clone(),
                size: asset.size,
                format: selection.format,
            });
        }
    }
    Err(ArtifactError::invalid(format!(
        "release {version} has no supported asset; tried {}",
        selection.names.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use sempre_core::{BuiltInAdapter, BuiltInKind};

    use super::*;

    #[test]
    fn endpoint_encodes_versions_and_rejects_invalid_repositories() {
        let base = Url::parse("https://api.github.com").expect("URL");
        let endpoint =
            release_endpoint(&base, "SagerNet/sing-box", "1.13.0-alpha.1").expect("endpoint");
        assert_eq!(
            endpoint.as_str(),
            "https://api.github.com/repos/SagerNet/sing-box/releases/tags/v1.13.0-alpha.1"
        );
        for repository in ["", "owner", "a/b/c", "../repo", "owner/.."] {
            assert!(release_endpoint(&base, repository, STABLE).is_err());
        }
    }

    #[test]
    fn package_selection_follows_adapter_priority_and_requires_digest() {
        let adapter = BuiltInAdapter::new(BuiltInKind::Mihomo);
        let target = Target {
            os: "linux".into(),
            arch: "amd64".into(),
            amd64_level: 3,
        };
        let mut release = Release {
            tag: "v1.19.29".into(),
            draft: false,
            prerelease: false,
            assets: vec![ReleaseAsset {
                name: "mihomo-linux-amd64-v2-v1.19.29.gz".into(),
                url: "https://example.invalid/core.gz".into(),
                digest: format!("sha256:{}", "a".repeat(64)),
                size: 42,
                created_at: String::new(),
            }],
        };
        let package = package_from_release(&adapter, &release, &target).expect("package");
        assert_eq!(package.name, "mihomo-linux-amd64-v2-v1.19.29.gz");
        release.assets[0].digest = "sha256:00".into();
        assert!(package_from_release(&adapter, &release, &target).is_err());
    }
}
