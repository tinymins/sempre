use std::{io, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SubscriptionError {
    #[error("open subscription lock {path}: {source}")]
    OpenLock { path: PathBuf, source: io::Error },
    #[error("lock subscription catalog: {0}")]
    Lock(#[source] io::Error),
    #[error("read subscription catalog: {0}")]
    Read(#[source] io::Error),
    #[error("decode subscription catalog: {0}")]
    Decode(#[source] serde_json::Error),
    #[error("encode subscription catalog: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("write subscription catalog: {0}")]
    Write(#[source] io::Error),
    #[error("invalid subscription catalog: {0}")]
    Invalid(String),
    #[error("subscription source exceeds {limit} bytes")]
    SourceTooLarge { limit: usize },
    #[error("invalid subscription content hash {0:?}")]
    InvalidHash(String),
    #[error("read subscription snapshot: {0}")]
    ReadSnapshot(#[source] io::Error),
    #[error("subscription snapshot {hash} failed integrity verification")]
    SnapshotIntegrity { hash: String },
    #[error("write subscription snapshot: {0}")]
    WriteSnapshot(#[source] io::Error),
    #[error("subscription fetch failed: {0}")]
    Fetch(String),
    #[error("read subscription cache: {0}")]
    ReadCache(#[source] io::Error),
    #[error("decode subscription cache: {0}")]
    DecodeCache(#[source] serde_json::Error),
    #[error("encode subscription cache: {0}")]
    EncodeCache(#[source] serde_json::Error),
    #[error("write subscription cache: {0}")]
    WriteCache(#[source] io::Error),
}
