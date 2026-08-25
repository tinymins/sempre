use std::{io, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TunnelError {
    #[error("{0}")]
    Invalid(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Artifact(#[from] sempre_artifact::ArtifactError),
    #[error(transparent)]
    Supervisor(#[from] sempre_supervisor::SupervisorError),
    #[error("tunnel worker {0} failed")]
    Worker(#[source] tokio::task::JoinError),
    #[error("path is not valid Unicode: {0}")]
    NonUnicodePath(PathBuf),
}

impl TunnelError {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    pub(crate) fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}
