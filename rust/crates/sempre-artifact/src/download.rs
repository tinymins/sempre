use std::{path::Path, str::FromStr, time::Duration};

use futures_util::StreamExt;
use reqwest::{Client, StatusCode, header};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use url::Url;

use crate::{ArtifactError, RemoveOnDrop, Result, Sha256Digest, https_redirect_policy};

pub const MAX_ARTIFACT_SIZE: u64 = 512 << 20;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    pub name: String,
    pub url: String,
    pub digest: String,
    pub size: u64,
}

#[derive(Clone)]
pub struct Downloader {
    client: Client,
}

impl Downloader {
    pub fn new(user_agent: &str) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_mins(15))
            .redirect(https_redirect_policy())
            .user_agent(user_agent)
            .build()
            .map_err(|error| ArtifactError::http("build download client", error))?;
        Ok(Self { client })
    }

    pub async fn verified(&self, artifact: &Artifact, destination: &Path) -> Result<()> {
        let expected = validate_artifact(artifact)?;
        let response = self
            .client
            .get(&artifact.url)
            .send()
            .await
            .map_err(|error| ArtifactError::http(format!("download {}", artifact.name), error))?;
        if response.status() != StatusCode::OK {
            return Err(ArtifactError::invalid(format!(
                "download {}: HTTP {}",
                artifact.name,
                response.status()
            )));
        }
        if let Some(length) = response.headers().get(header::CONTENT_LENGTH) {
            let length = length
                .to_str()
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .ok_or_else(|| {
                    ArtifactError::invalid(format!(
                        "{} has an invalid Content-Length",
                        artifact.name
                    ))
                })?;
            if length != artifact.size {
                return Err(ArtifactError::invalid(format!(
                    "{} size changed: expected {}, server reports {length}",
                    artifact.name, artifact.size
                )));
            }
        }

        let file = create_destination(destination).await?;
        let cleanup = RemoveOnDrop::new(destination.to_path_buf());
        let body = response.bytes_stream().map(|chunk| {
            chunk.map_err(|error| ArtifactError::http(format!("download {}", artifact.name), error))
        });
        write_verified_stream(body, file, artifact, &expected).await?;
        cleanup.keep();
        Ok(())
    }
}

async fn write_verified_stream<S, B>(
    mut body: S,
    mut file: tokio::fs::File,
    artifact: &Artifact,
    expected: &Sha256Digest,
) -> Result<()>
where
    S: futures_util::Stream<Item = Result<B>> + Unpin,
    B: AsRef<[u8]>,
{
    let mut hash = Sha256::new();
    let mut written = 0_u64;
    while let Some(chunk) = body.next().await {
        let chunk = chunk?;
        let data = chunk.as_ref();
        written = written
            .checked_add(data.len() as u64)
            .ok_or_else(|| ArtifactError::invalid("artifact size overflow"))?;
        if written > MAX_ARTIFACT_SIZE || written > artifact.size {
            return Err(ArtifactError::invalid(format!(
                "{} exceeds its declared size",
                artifact.name
            )));
        }
        hash.update(data);
        file.write_all(data)
            .await
            .map_err(|error| ArtifactError::io("write download", error))?;
    }
    if written != artifact.size {
        return Err(ArtifactError::invalid(format!(
            "{} size mismatch: expected {}, got {written}",
            artifact.name, artifact.size
        )));
    }
    let actual = Sha256Digest::from_bytes(hash.finalize().into());
    if &actual != expected {
        return Err(ArtifactError::invalid(format!(
            "{} SHA-256 mismatch: expected {expected}, got {actual}",
            artifact.name
        )));
    }
    file.flush()
        .await
        .map_err(|error| ArtifactError::io("flush download", error))?;
    file.sync_all()
        .await
        .map_err(|error| ArtifactError::io("sync download", error))?;
    drop(file);
    Ok(())
}

fn validate_artifact(artifact: &Artifact) -> Result<Sha256Digest> {
    let digest = Sha256Digest::from_str(&artifact.digest)?;
    let url = Url::parse(&artifact.url).map_err(|_| {
        ArtifactError::invalid(format!("{} has an invalid HTTPS URL", artifact.name))
    })?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(ArtifactError::invalid(format!(
            "{} has an invalid HTTPS URL",
            artifact.name
        )));
    }
    if artifact.size == 0 || artifact.size > MAX_ARTIFACT_SIZE {
        return Err(ArtifactError::invalid(format!(
            "{} has invalid size {}",
            artifact.name, artifact.size
        )));
    }
    Ok(digest)
}

async fn create_destination(path: &Path) -> Result<tokio::fs::File> {
    let mut options = tokio::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    options
        .open(path)
        .await
        .map_err(|error| ArtifactError::io("create download", error))
}

#[cfg(test)]
mod tests {
    use futures_util::stream;
    use sha2::Digest;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn rejects_untrusted_metadata_before_downloading() {
        let valid = Artifact {
            name: "core.zip".into(),
            url: "https://example.invalid/core.zip".into(),
            digest: format!("sha256:{}", "a".repeat(64)),
            size: 42,
        };
        assert!(validate_artifact(&valid).is_ok());
        for artifact in [
            Artifact {
                url: "http://example.invalid/core.zip".into(),
                ..valid.clone()
            },
            Artifact {
                digest: "sha256:00".into(),
                ..valid.clone()
            },
            Artifact {
                size: 0,
                ..valid.clone()
            },
            Artifact {
                size: MAX_ARTIFACT_SIZE + 1,
                ..valid.clone()
            },
        ] {
            assert!(validate_artifact(&artifact).is_err());
        }
    }

    #[tokio::test]
    async fn streamed_write_requires_exact_size_and_digest() {
        let root = tempdir().expect("temporary directory");
        let payload = b"verified core";
        let digest = Sha256Digest::from_bytes(Sha256::digest(payload).into());
        let artifact = Artifact {
            name: "core".into(),
            url: "https://example.invalid/core".into(),
            digest: digest.to_string(),
            size: payload.len() as u64,
        };
        let output = root.path().join("output");
        let file = create_destination(&output).await.expect("destination");
        write_verified_stream(
            stream::iter([Ok::<_, ArtifactError>(&payload[..4]), Ok(&payload[4..])]),
            file,
            &artifact,
            &digest,
        )
        .await
        .expect("verified stream");
        assert_eq!(tokio::fs::read(&output).await.expect("output"), payload);

        let short_output = root.path().join("short");
        let short_file = create_destination(&short_output)
            .await
            .expect("destination");
        assert!(
            write_verified_stream(
                stream::iter([Ok::<_, ArtifactError>(&payload[..4])]),
                short_file,
                &artifact,
                &digest,
            )
            .await
            .is_err()
        );
    }
}
