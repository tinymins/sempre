use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::Utc;
use sempre_state::AppliedMigration;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    api,
    api::AppState,
    traffic_history_migrations,
    traffic_rotation::{self, RotationError, TrafficSettings},
};

#[cfg(test)]
use crate::traffic_rotation::{MAX_RETENTION_HOURS, MIN_MAX_BYTES};

const BUCKET_MILLIS: i64 = 60_000;
#[derive(Debug, Error)]
pub(crate) enum TrafficError {
    #[error("read traffic history {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("decode traffic history {path}: {source}")]
    Decode {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("encode traffic history: {0}")]
    Encode(#[from] serde_json::Error),
    #[error("write traffic history {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("traffic history lock is poisoned")]
    Lock,
    #[error("traffic history schema {0} is not supported")]
    Schema(u32),
    #[error(transparent)]
    Migration(#[from] sempre_state::MigrationError),
    #[error(transparent)]
    Rotation(#[from] RotationError),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TrafficDimension {
    Device,
    User,
    Host,
    Outbound,
    Process,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredRecord {
    time: i64,
    dimension: TrafficDimension,
    label: String,
    download: i64,
    upload: i64,
}
#[derive(Debug, Deserialize, Serialize)]
struct Document {
    schema: u32,
    applied_migrations: Vec<AppliedMigration>,
    settings: TrafficSettings,
    records: Vec<StoredRecord>,
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RecordKey {
    time: i64,
    dimension: TrafficDimension,
    label: String,
}
#[derive(Clone, Copy, Debug, Default)]
struct Totals {
    download: i64,
    upload: i64,
}

struct Inner {
    settings: TrafficSettings,
    records: HashMap<RecordKey, Totals>,
    dirty: bool,
}

pub(crate) struct TrafficStore {
    path: PathBuf,
    inner: Mutex<Inner>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrafficTotal {
    label: String,
    download: i64,
    upload: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrafficHistory {
    settings: TrafficSettings,
    storage_bytes: usize,
    totals: Vec<TrafficTotal>,
}

impl TrafficStore {
    pub(crate) fn open(path: PathBuf) -> Result<Self, TrafficError> {
        let (document, migrated) = match fs::read(&path) {
            Ok(data) => {
                let migration = traffic_history_migrations::run(&data)?;
                let document =
                    serde_json::from_value::<Document>(migration.value).map_err(|source| {
                        TrafficError::Decode {
                            path: path.clone(),
                            source,
                        }
                    })?;
                (document, migration.changed)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
                Document {
                    schema: traffic_history_migrations::CURRENT_SCHEMA,
                    applied_migrations: traffic_history_migrations::current_ledger(),
                    settings: TrafficSettings::default(),
                    records: Vec::new(),
                },
                false,
            ),
            Err(source) => {
                return Err(TrafficError::Read {
                    path: path.clone(),
                    source,
                });
            }
        };
        if document.schema != traffic_history_migrations::CURRENT_SCHEMA {
            return Err(TrafficError::Schema(document.schema));
        }
        traffic_history_migrations::validate_ledger(&document.applied_migrations)?;
        traffic_rotation::validate(&document.settings)?;
        if migrated {
            write_document(&path, &document)?;
        }
        let records = document
            .records
            .into_iter()
            .map(|record| {
                (
                    RecordKey {
                        time: record.time,
                        dimension: record.dimension,
                        label: record.label,
                    },
                    Totals {
                        download: record.download,
                        upload: record.upload,
                    },
                )
            })
            .collect();
        Ok(Self {
            path,
            inner: Mutex::new(Inner {
                settings: document.settings,
                records,
                dirty: false,
            }),
        })
    }

    pub(crate) fn record(&self, time: i64, deltas: Vec<TrafficDelta>) -> Result<(), TrafficError> {
        if deltas.is_empty() {
            return Ok(());
        }
        let mut inner = self.lock()?;
        let bucket = time - time.rem_euclid(BUCKET_MILLIS);
        for delta in deltas {
            let totals = inner
                .records
                .entry(RecordKey {
                    time: bucket,
                    dimension: delta.dimension,
                    label: delta.label,
                })
                .or_default();
            totals.download += delta.download;
            totals.upload += delta.upload;
        }
        rotate(&mut inner, time);
        inner.dirty = true;
        Ok(())
    }

    pub(crate) fn history(
        &self,
        since: i64,
        dimension: TrafficDimension,
        now: i64,
    ) -> Result<TrafficHistory, TrafficError> {
        let inner = self.lock()?;
        let since = traffic_rotation::summary_cutoff(&inner.settings, now)
            .map_or(since, |cutoff| since.max(cutoff));
        let mut totals = HashMap::<String, Totals>::new();
        for (key, value) in &inner.records {
            if key.time < since || key.dimension != dimension {
                continue;
            }
            let total = totals.entry(key.label.clone()).or_default();
            total.download += value.download;
            total.upload += value.upload;
        }
        let mut totals = totals
            .into_iter()
            .map(|(label, total)| TrafficTotal {
                label,
                download: total.download,
                upload: total.upload,
            })
            .collect::<Vec<_>>();
        totals.sort_by(|left, right| {
            (right.download + right.upload).cmp(&(left.download + left.upload))
        });
        Ok(TrafficHistory {
            settings: inner.settings.clone(),
            storage_bytes: encoded(&inner)?.len(),
            totals,
        })
    }

    pub(crate) fn update_settings(
        &self,
        settings: TrafficSettings,
        now: i64,
    ) -> Result<(), TrafficError> {
        traffic_rotation::validate(&settings)?;
        let mut inner = self.lock()?;
        inner.settings = settings;
        rotate(&mut inner, now);
        inner.dirty = true;
        persist(&self.path, &mut inner)
    }

    pub(crate) fn clear(&self) -> Result<(), TrafficError> {
        let mut inner = self.lock()?;
        inner.records.clear();
        inner.dirty = true;
        persist(&self.path, &mut inner)
    }

    pub(crate) fn flush(&self) -> Result<(), TrafficError> {
        let mut inner = self.lock()?;
        if inner.dirty {
            persist(&self.path, &mut inner)?;
        }
        Ok(())
    }

    pub(crate) fn maintain(&self, now: i64) -> Result<(), TrafficError> {
        let mut inner = self.lock()?;
        if rotate(&mut inner, now) {
            inner.dirty = true;
        }
        if inner.dirty {
            persist(&self.path, &mut inner)?;
        }
        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, Inner>, TrafficError> {
        self.inner.lock().map_err(|_| TrafficError::Lock)
    }
}

fn rotate(inner: &mut Inner, now: i64) -> bool {
    let Some(cutoff) = traffic_rotation::storage_cutoff(&inner.settings, now) else {
        return false;
    };
    let previous = inner.records.len();
    inner.records.retain(|key, _| key.time >= cutoff);
    inner.records.len() != previous
}

fn persist(path: &Path, inner: &mut Inner) -> Result<(), TrafficError> {
    let mut data = encoded(inner)?;
    while inner
        .settings
        .max_bytes
        .is_some_and(|limit| data.len() as u64 > limit)
        && !inner.records.is_empty()
    {
        let oldest = inner
            .records
            .keys()
            .map(|key| key.time)
            .min()
            .unwrap_or_default();
        inner.records.retain(|key, _| key.time > oldest);
        data = encoded(inner)?;
    }
    sempre_state::write_atomic(path, &data, 0o600).map_err(|source| TrafficError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    inner.dirty = false;
    Ok(())
}

fn write_document(path: &Path, document: &Document) -> Result<(), TrafficError> {
    let data = serde_json::to_vec(document)?;
    sempre_state::write_atomic(path, &data, 0o600).map_err(|source| TrafficError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn encoded(inner: &Inner) -> Result<Vec<u8>, TrafficError> {
    let mut records = inner
        .records
        .iter()
        .map(|(key, totals)| StoredRecord {
            time: key.time,
            dimension: key.dimension,
            label: key.label.clone(),
            download: totals.download,
            upload: totals.upload,
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| {
        left.time
            .cmp(&right.time)
            .then_with(|| left.label.cmp(&right.label))
    });
    Ok(serde_json::to_vec(&Document {
        schema: traffic_history_migrations::CURRENT_SCHEMA,
        applied_migrations: traffic_history_migrations::current_ledger(),
        settings: inner.settings.clone(),
        records,
    })?)
}

#[derive(Debug)]
pub(crate) struct TrafficDelta {
    pub(crate) dimension: TrafficDimension,
    pub(crate) label: String,
    pub(crate) download: i64,
    pub(crate) upload: i64,
}

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route(
        "/api/v1/runtime/traffic/history",
        get(history).patch(update_settings).delete(clear),
    )
}

#[derive(Deserialize)]
struct HistoryQuery {
    #[serde(default)]
    since: i64,
    #[serde(default = "default_dimension")]
    dimension: TrafficDimension,
}

fn default_dimension() -> TrafficDimension {
    TrafficDimension::Host
}

async fn history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    result(
        state
            .traffic
            .history(query.since, query.dimension, Utc::now().timestamp_millis()),
    )
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<TrafficSettings>,
) -> Response {
    match state
        .traffic
        .update_settings(settings, Utc::now().timestamp_millis())
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(TrafficError::Rotation(error)) => api::api_error(
            StatusCode::BAD_REQUEST,
            "INVALID_TRAFFIC_SETTINGS",
            error.to_string(),
        ),
        Err(error) => internal_error(&error),
    }
}

async fn clear(State(state): State<Arc<AppState>>) -> Response {
    match state.traffic.clear() {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => internal_error(&error),
    }
}

fn result(value: Result<TrafficHistory, TrafficError>) -> Response {
    value.map_or_else(
        |error| internal_error(&error),
        |history| Json(history).into_response(),
    )
}

fn internal_error(error: &TrafficError) -> Response {
    api::api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "TRAFFIC_HISTORY_ERROR",
        error.to_string(),
    )
}

#[cfg(test)]
#[path = "traffic_history_tests.rs"]
mod tests;
