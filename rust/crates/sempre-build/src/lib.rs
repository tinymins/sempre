mod checksum;
mod cores;
mod package;
mod target;
mod ui;

use std::{io, path::PathBuf};

use thiserror::Error;

pub use package::{BuildInput, BuildOutput, package};
pub use target::BuildTarget;
pub use ui::prepare_ui;

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("invalid build input: {0}")]
    Invalid(String),
    #[error("{operation} {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("decode {name}: {source}")]
    Decode {
        name: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error(transparent)]
    Artifact(#[from] sempre_artifact::ArtifactError),
    #[error(transparent)]
    Bundle(#[from] sempre_bundle::BundleError),
    #[error(transparent)]
    Control(#[from] sempre_control::ControlError),
    #[error(transparent)]
    Core(#[from] sempre_core::RegistryError),
    #[error(transparent)]
    State(#[from] sempre_state::StateError),
    #[error(transparent)]
    Subscription(#[from] sempre_subscription::SubscriptionError),
    #[error(transparent)]
    Tunnel(#[from] sempre_tunnel::TunnelError),
    #[error(transparent)]
    Ui(#[from] sempre_ui::UiError),
    #[error("run {program}: {source}")]
    Start {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error("{program} failed with exit code {code}")]
    Command { program: String, code: i32 },
}

impl BuildError {
    pub fn io(operation: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            source,
        }
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}
