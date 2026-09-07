use std::{
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use futures_util::StreamExt;
use reqwest::{Client, Proxy, StatusCode, header};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

use crate::{ArtifactError, RemoveOnDrop, Result, Sha256Digest};

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
    cache: Option<PathBuf>,
}

impl Downloader {
    pub fn new(user_agent: &str) -> Result<Self> {
        let client = crate::http::client_builder(user_agent, Duration::from_mins(15))
            .build()
            .map_err(|error| ArtifactError::http("build download client", error))?;
        Ok(Self {
            client,
            cache: None,
        })
    }

    pub fn new_via_http_proxy(
        user_agent: &str,
        address: std::net::SocketAddr,
        username: &str,
        password: &str,
    ) -> Result<Self> {
        let proxy = Proxy::all(format!("http://{address}"))
            .map_err(|error| ArtifactError::http("configure download proxy", error))?
            .basic_auth(username, password);
        let client = crate::http::client_builder(user_agent, Duration::from_mins(15))
            .proxy(proxy)
            .build()
            .map_err(|error| ArtifactError::http("build proxied download client", error))?;
        Ok(Self {
            client,
            cache: None,
        })
    }

    #[must_use]
    pub fn with_cache(mut self, directory: impl Into<PathBuf>) -> Self {
        self.cache = Some(directory.into());
        self
    }

    pub async fn verified(&self, artifact: &Artifact, destination: &Path) -> Result<()> {
        self.verified_with_progress(artifact, destination, |_, _| {})
            .await
    }

    pub async fn verified_with_progress(
        &self,
        artifact: &Artifact,
        destination: &Path,
        progress: impl Fn(u64, u64),
    ) -> Result<()> {
        let expected = validate_artifact(artifact)?;
        if let Some(cache) = &self.cache {
            let cached = cache.join(expected.to_string().trim_start_matches("sha256:"));
            if cached.is_file() {
                if verify_cached(&cached, artifact, &expected).await? {
                    copy_cached(&cached, destination).await?;
                    return Ok(());
                }
                tokio::fs::remove_file(&cached)
                    .await
                    .map_err(|error| ArtifactError::io("remove invalid cached artifact", error))?;
            }
        }
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
        write_verified_stream(body, file, artifact, &expected, progress).await?;
        cleanup.keep();
        if let Some(cache) = &self.cache {
            tokio::fs::create_dir_all(cache)
                .await
                .map_err(|error| ArtifactError::io("create artifact cache", error))?;
            let cached = cache.join(expected.to_string().trim_start_matches("sha256:"));
            tokio::fs::copy(destination, &cached)
                .await
                .map_err(|error| ArtifactError::io("cache verified artifact", error))?;
        }
        Ok(())
    }
}

async fn verify_cached(path: &Path, artifact: &Artifact, expected: &Sha256Digest) -> Result<bool> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| ArtifactError::io("inspect cached artifact", error))?;
    if metadata.len() != artifact.size {
        return Ok(false);
    }
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| ArtifactError::io("open cached artifact", error))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 64 << 10];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| ArtifactError::io("read cached artifact", error))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(Sha256Digest::from_bytes(hash.finalize().into()) == *expected)
}

async fn copy_cached(source: &Path, destination: &Path) -> Result<()> {
    let mut source = tokio::fs::File::open(source)
        .await
        .map_err(|error| ArtifactError::io("open cached artifact", error))?;
    let mut output = create_destination(destination).await?;
    let cleanup = RemoveOnDrop::new(destination.to_path_buf());
    tokio::io::copy(&mut source, &mut output)
        .await
        .map_err(|error| ArtifactError::io("copy cached artifact", error))?;
    output
        .sync_all()
        .await
        .map_err(|error| ArtifactError::io("sync cached artifact", error))?;
    cleanup.keep();
    Ok(())
}

async fn write_verified_stream<S, B>(
    mut body: S,
    mut file: tokio::fs::File,
    artifact: &Artifact,
    expected: &Sha256Digest,
    progress: impl Fn(u64, u64),
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
        progress(written, artifact.size);
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
    use tokio::{io::AsyncReadExt as _, io::AsyncWriteExt as _, net::TcpListener};

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
        let progress = std::cell::Cell::new((0, 0));
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
            |downloaded, total| progress.set((downloaded, total)),
        )
        .await
        .expect("verified stream");
        assert_eq!(tokio::fs::read(&output).await.expect("output"), payload);
        assert_eq!(progress.get(), (payload.len() as u64, payload.len() as u64));

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
                |_, _| {},
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn verified_download_reuses_valid_content_addressed_cache_entries() {
        let root = tempdir().expect("temporary directory");
        let payload = b"verified core";
        let digest = Sha256Digest::from_bytes(Sha256::digest(payload).into());
        let artifact = Artifact {
            name: "core".into(),
            url: "https://example.invalid/core".into(),
            digest: digest.to_string(),
            size: payload.len() as u64,
        };
        let cache = root.path().join("cache");
        tokio::fs::create_dir_all(&cache).await.expect("cache");
        let cached = cache.join(digest.to_string().trim_start_matches("sha256:"));
        tokio::fs::write(&cached, payload)
            .await
            .expect("cached artifact");

        let output = root.path().join("output");
        Downloader::new("test")
            .expect("downloader")
            .with_cache(&cache)
            .verified(&artifact, &output)
            .await
            .expect("cache hit");
        assert_eq!(tokio::fs::read(output).await.expect("output"), payload);

        tokio::fs::write(&cached, b"corrupt core")
            .await
            .expect("corrupt cache");
        assert!(
            !verify_cached(&cached, &artifact, &digest)
                .await
                .expect("inspect invalid cache")
        );
    }

    #[tokio::test]
    async fn proxied_downloader_uses_authenticated_http_connect() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let request = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0_u8; 4096];
            let read = socket.read(&mut buffer).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
            String::from_utf8_lossy(&buffer[..read]).into_owned()
        });
        let artifact = Artifact {
            name: "release.zip".into(),
            url: "https://example.invalid/release.zip".into(),
            digest: format!("sha256:{}", "a".repeat(64)),
            size: 1,
        };
        let root = tempdir().unwrap();

        let error = Downloader::new_via_http_proxy("test", address, "sempre", "secret")
            .unwrap()
            .verified(&artifact, &root.path().join("release.zip"))
            .await
            .unwrap_err();
        let request = request.await.unwrap();

        assert!(matches!(error, ArtifactError::Http { .. }));
        assert!(request.starts_with("CONNECT example.invalid:443 HTTP/1.1\r\n"));
        assert!(request.contains("proxy-authorization: Basic c2VtcHJlOnNlY3JldA==\r\n"));
    }
}
