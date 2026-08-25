mod archive;
mod digest;
mod download;
mod github;

use std::{io, path::PathBuf};

use thiserror::Error;

pub use archive::{ArchiveFormat, ExtractOptions, extract, find};
pub use digest::Sha256Digest;
pub use download::{Artifact, Downloader, MAX_ARTIFACT_SIZE};
pub use github::{GithubClient, Release, ReleaseAsset};

pub type Result<T> = std::result::Result<T, ArtifactError>;

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("{0}")]
    Invalid(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    #[error("{context}: {source}")]
    Http {
        context: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("{context}: {source}")]
    Zip {
        context: String,
        #[source]
        source: zip::result::ZipError,
    },
}

impl ArtifactError {
    fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    fn http(context: impl Into<String>, source: reqwest::Error) -> Self {
        Self::Http {
            context: context.into(),
            source,
        }
    }

    fn zip(context: impl Into<String>, source: zip::result::ZipError) -> Self {
        Self::Zip {
            context: context.into(),
            source,
        }
    }
}

struct RemoveOnDrop(Option<PathBuf>);

impl RemoveOnDrop {
    fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    fn keep(mut self) {
        self.0 = None;
    }
}

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn https_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            attempt.error("too many redirects")
        } else if attempt.url().scheme() != "https" {
            attempt.error("refuse non-HTTPS redirect")
        } else {
            attempt.follow()
        }
    })
}
