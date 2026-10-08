mod github;
mod install;

pub use install::PreparedInstallation;

use std::{fs, io, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MANIFEST_NAME: &str = "sempre-ui.json";
pub const METADATA_NAME: &str = ".sempre-source.json";
pub const MAX_ARCHIVE_SIZE: usize = 64 << 20;
pub const MAX_EXPANDED_SIZE: u64 = 128 << 20;
pub const MAX_EXTRACTED_FILES: usize = 4096;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Manifest {
    pub schema: u32,
    pub name: String,
    pub version: String,
    pub entry: String,
    pub api: Api,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Api {
    pub major: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Metadata {
    pub manifest: Manifest,
    pub source_type: String,
    pub source: String,
    #[serde(rename = "sha256")]
    pub digest: String,
    pub installed_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum UiError {
    #[error("read UI metadata: {0}")]
    Read(#[source] io::Error),
    #[error("decode UI metadata: {0}")]
    Decode(#[source] serde_json::Error),
    #[error("invalid UI installation: {0}")]
    Invalid(String),
    #[error("write UI installation: {0}")]
    Write(#[source] io::Error),
    #[error("download UI: {0}")]
    Http(#[source] reqwest::Error),
    #[error("resolve UI release: {0}")]
    Artifact(#[from] sempre_artifact::ArtifactError),
    #[error("read UI ZIP: {0}")]
    Zip(#[source] zip::result::ZipError),
}

#[derive(Clone, Debug)]
pub struct Store {
    pub(crate) root: PathBuf,
}

impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn current_dir(&self) -> PathBuf {
        self.root.join("current")
    }

    pub fn current(&self) -> Result<Metadata, UiError> {
        let current = self.current_dir();
        let data = fs::read(current.join(METADATA_NAME)).map_err(UiError::Read)?;
        let metadata: Metadata = serde_json::from_slice(&data).map_err(UiError::Decode)?;
        validate(&metadata.manifest)?;
        let entry = current.join(&metadata.manifest.entry);
        if !entry.is_file() {
            return Err(UiError::Invalid(format!(
                "UI entry {:?} is unavailable",
                metadata.manifest.entry
            )));
        }
        Ok(metadata)
    }
}

pub(crate) fn validate(manifest: &Manifest) -> Result<(), UiError> {
    if manifest.schema != 1 || manifest.api.major != 1 {
        return Err(UiError::Invalid(
            "UI manifest is incompatible with Sempre API v1".into(),
        ));
    }
    if manifest.name.trim().is_empty() || manifest.version.trim().is_empty() {
        return Err(UiError::Invalid(
            "UI manifest name and version are required".into(),
        ));
    }
    if manifest.entry != "index.html" {
        return Err(UiError::Invalid(
            "UI manifest entry must be index.html".into(),
        ));
    }
    Ok(())
}
