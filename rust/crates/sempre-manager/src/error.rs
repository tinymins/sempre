use std::{io, path::PathBuf};

use sempre_artifact::ArtifactError;
use sempre_control::ControlError;
use sempre_core::{ReferenceError, RegistryError};
use sempre_state::{LayoutError, StateError};
use sempre_subscription::SubscriptionError;
use sempre_supervisor::SupervisorError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error(transparent)]
    Reference(#[from] ReferenceError),
    #[error(transparent)]
    Core(#[from] RegistryError),
    #[error(transparent)]
    Control(#[from] ControlError),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error(transparent)]
    Bundle(#[from] sempre_bundle::BundleError),
    #[error(transparent)]
    Service(#[from] sempre_service::ServiceError),
    #[error(transparent)]
    Subscription(#[from] SubscriptionError),
    #[error(transparent)]
    Gateway(#[from] sempre_gateway::GatewayError),
    #[error(transparent)]
    Network(#[from] sempre_network::NetworkError),
    #[error("compile subscription profile: {0}")]
    Compile(#[from] sempre_converter::CompileError),
    #[error(transparent)]
    Supervisor(#[from] SupervisorError),
    #[error(transparent)]
    Transparent(#[from] sempre_transparent::TransparentError),
    #[error(transparent)]
    Tunnel(#[from] sempre_tunnel::TunnelError),
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
    #[error("foreground core {reference} exited with {status}")]
    DirectExit { reference: String, status: String },
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
    #[error("subscription profile {0:?} was not found")]
    ProfileNotFound(String),
    #[error("runtime is not ready: {0}")]
    RuntimeNotReady(String),
    #[error("{0}")]
    InvalidOperation(String),
    #[error("replacing the existing system deployment requires --yes: {0}")]
    ConfirmationRequired(String),
    #[error("subscription configuration target changed; reload before saving")]
    ConfigurationContextChanged,
    #[error("application uninstall incomplete: {0}")]
    UninstallIncomplete(String),
    #[error("{message}")]
    RuntimeAction { code: &'static str, message: String },
}

impl ManagerError {
    pub(crate) fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    pub fn runtime_action_code(&self) -> Option<&'static str> {
        match self {
            Self::RuntimeAction { code, .. } => Some(code),
            _ => None,
        }
    }
}
