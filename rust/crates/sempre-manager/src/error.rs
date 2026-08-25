use std::{io, path::PathBuf};

use sempre_artifact::ArtifactError;
use sempre_core::{ReferenceError, RegistryError};
use sempre_state::StateError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    Reference(#[from] ReferenceError),
    #[error(transparent)]
    Core(#[from] RegistryError),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    #[error("path is not valid Unicode: {0}")]
    NonUnicodePath(PathBuf),
    #[error("{core} version command failed with {status}: {output}")]
    VersionCommand {
        core: String,
        status: String,
        output: String,
    },
    #[error("{0} version command timed out after 30 seconds")]
    VersionTimeout(String),
    #[error("downloaded {core} reports version {actual}, expected {expected}")]
    VersionMismatch {
        core: String,
        expected: String,
        actual: String,
    },
    #[error(
        "{reference} is already installed from {existing}; remove it before installing {candidate}"
    )]
    ConflictingSource {
        reference: String,
        existing: String,
        candidate: String,
    },
}

impl ManagerError {
    pub(crate) fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}
