use std::path::Path;

use crate::{ServiceError, State};

pub async fn status() -> Result<State, ServiceError> {
    Ok(State::Unknown)
}

pub async fn install(_: &Path, _: &Path) -> Result<(), ServiceError> {
    Err(ServiceError::InvalidAction)
}

pub async fn uninstall() -> Result<(), ServiceError> {
    Err(ServiceError::InvalidAction)
}

pub async fn start() -> Result<(), ServiceError> {
    Err(ServiceError::InvalidAction)
}

pub async fn stop() -> Result<(), ServiceError> {
    Err(ServiceError::InvalidAction)
}

pub async fn restart() -> Result<(), ServiceError> {
    Err(ServiceError::InvalidAction)
}
