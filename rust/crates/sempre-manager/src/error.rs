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
    #[error("{core} {operation} timed out after 30 seconds")]
    CommandTimeout {
        core: String,
        operation: &'static str,
    },
    #[error("{core} rejected the configuration with {status}: {output}")]
    ValidationCommand {
        core: String,
        status: String,
        output: String,
    },
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
    #[error("{0} is not installed; install it first")]
    NotInstalled(String),
    #[error("cannot remove {reference}: it is {usage}")]
    CoreInUse {
        reference: String,
        usage: &'static str,
    },
    #[error("core state changed while {operation} {reference}; retry the command")]
    CoreStateChanged {
        operation: &'static str,
        reference: String,
    },
    #[error("candidate {reference} rejected the active configuration: {source}")]
    CandidateRejected {
        reference: String,
        #[source]
        source: Box<ManagerError>,
    },
    #[error("no core is selected; select an installed core first")]
    NoSelectedCore,
    #[error("selected core has no configuration")]
    NoConfiguration,
    #[error("configuration exceeds {limit} bytes")]
    ConfigurationTooLarge { limit: usize },
}

impl ManagerError {
    pub(crate) fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}
