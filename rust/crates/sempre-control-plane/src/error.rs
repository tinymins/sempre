use std::{io, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ControlError {
    #[error("{0}")]
    Invalid(String),
    #[error("{context} {path}: {source}")]
    Io {
        context: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("encode control data: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("decode control data: {0}")]
    Decode(#[source] serde_json::Error),
    #[error("hash administrator password: {0}")]
    Password(String),
}

impl ControlError {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    pub(crate) fn io(context: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            context,
            path: path.into(),
            source,
        }
    }
}
