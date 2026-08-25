use std::time::Duration;

use futures_util::StreamExt as _;
use regex::Regex;
use reqwest::{Client, StatusCode};
use sempre_artifact::{GithubClient, ReleaseAsset, Sha256Digest};

use crate::{Store, UiError};

const ARCHIVE_NAME: &str = "sempre-ui.zip";
const CHECKSUM_NAME: &str = "SHA256SUMS";
const MAX_CHECKSUM_SIZE: usize = 1 << 20;

impl Store {
    pub async fn install_github(&self, value: &str) -> Result<crate::Metadata, UiError> {
        let reference = Reference::parse(value)?;
        let client = GithubClient::new(concat!("Sempre/", env!("CARGO_PKG_VERSION")))?;
        let release = client
            .release(&reference.repository, &reference.version)
            .await?;
        if reference.version == "stable" && release.prerelease {
            return Err(invalid(format!(
                "latest UI release {} is a prerelease",
                release.tag
            )));
        }
        let archive = asset(&release.assets, ARCHIVE_NAME)
            .ok_or_else(|| invalid(format!("UI release {} has no {ARCHIVE_NAME}", release.tag)))?;
        let digest = if let Ok(digest) = archive.digest.parse::<Sha256Digest>() {
            digest.to_string()
        } else {
            let checksums = asset(&release.assets, CHECKSUM_NAME).ok_or_else(|| {
                invalid(format!(
                    "UI release {} provides neither an asset digest nor {CHECKSUM_NAME}",
                    release.tag
                ))
            })?;
            checksum(checksums, ARCHIVE_NAME).await?
        };
        self.install_url(&archive.url, "github", &reference.to_string(), &digest)
            .await
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Reference {
    repository: String,
    version: String,
}

impl Reference {
    fn parse(value: &str) -> Result<Self, UiError> {
        let value = value.trim();
        if value.matches('@').count() > 1 {
            return Err(invalid("invalid UI GitHub reference"));
        }
        let (repository, version) = value.split_once('@').unwrap_or((value, "stable"));
        let repository_pattern =
            Regex::new(r"^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})/[A-Za-z0-9_.-]{1,100}$")
                .expect("valid repository pattern");
        if !repository_pattern.is_match(repository)
            || repository
                .split_once('/')
                .is_some_and(|(_, name)| matches!(name, "." | ".."))
        {
            return Err(invalid(format!(
                "invalid UI GitHub repository {repository:?}; expected owner/repository"
            )));
        }
        let version = version.strip_prefix('v').unwrap_or(version);
        let version_pattern = Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$")
            .expect("valid version pattern");
        if version != "stable" && !version_pattern.is_match(version) {
            return Err(invalid(format!(
                "invalid UI version or channel {version:?}"
            )));
        }
        Ok(Self {
            repository: repository.to_ascii_lowercase(),
            version: version.into(),
        })
    }
}

impl std::fmt::Display for Reference {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}@{}", self.repository, self.version)
    }
}

fn asset<'a>(assets: &'a [ReleaseAsset], name: &str) -> Option<&'a ReleaseAsset> {
    assets.iter().find(|asset| asset.name == name)
}

async fn checksum(asset: &ReleaseAsset, name: &str) -> Result<String, UiError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                attempt.error("too many redirects")
            } else if attempt.url().scheme() != "https" {
                attempt.error("refuse non-HTTPS redirect")
            } else {
                attempt.follow()
            }
        }))
        .user_agent(concat!("Sempre/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(UiError::Http)?;
    let response = client.get(&asset.url).send().await.map_err(UiError::Http)?;
    if response.status() != StatusCode::OK {
        return Err(invalid(format!(
            "download UI checksums: HTTP {}",
            response.status()
        )));
    }
    let mut data = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(UiError::Http)?;
        if data.len().saturating_add(chunk.len()) > MAX_CHECKSUM_SIZE {
            return Err(invalid("UI checksums exceed 1 MiB"));
        }
        data.extend_from_slice(&chunk);
    }
    let body = String::from_utf8(data).map_err(|_| invalid("UI checksums are not UTF-8"))?;
    checksum_from_body(&body, name)
}

fn checksum_from_body(body: &str, name: &str) -> Result<String, UiError> {
    for line in body.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() == 2 && fields[1].trim_start_matches('*') == name {
            let digest = format!("sha256:{}", fields[0])
                .parse::<Sha256Digest>()
                .map_err(|_| invalid(format!("invalid checksum for {name}")))?;
            return Ok(digest.to_string());
        }
    }
    Err(invalid(format!("checksum for {name} is missing")))
}

fn invalid(message: impl Into<String>) -> UiError {
    UiError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ui_references_without_ambiguous_versions() {
        assert_eq!(
            Reference::parse("TinyMins/Sempre-UI").expect("stable reference"),
            Reference {
                repository: "tinymins/sempre-ui".into(),
                version: "stable".into(),
            }
        );
        assert_eq!(
            Reference::parse("tinymins/sempre-ui@v1.2.3-beta.1")
                .expect("version reference")
                .version,
            "1.2.3-beta.1"
        );
        for invalid in ["owner", "a/b/c", "owner/repo@next", "owner/repo@1@2"] {
            assert!(Reference::parse(invalid).is_err());
        }
    }

    #[test]
    fn extracts_typed_digest_from_release_checksums() {
        let digest = "a".repeat(64);
        assert_eq!(
            checksum_from_body(&format!("{digest}  {ARCHIVE_NAME}\n"), ARCHIVE_NAME)
                .expect("checksum"),
            format!("sha256:{digest}")
        );
        assert!(checksum_from_body("not-a-checksum  sempre-ui.zip", ARCHIVE_NAME).is_err());
    }
}
