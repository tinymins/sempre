use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
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

const BUCKET_MILLIS: i64 = 60_000;
/// Samples arrive often; rewriting the whole file for each would dominate idle CPU.
const PERSIST_INTERVAL: Duration = Duration::from_mins(1);
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
    applied_migrations: Vec<AppliedMigration>,
    settings: TrafficSettings,
    records: HashMap<RecordKey, Totals>,
    dirty: bool,
    persisted_bytes: usize,
    last_persist: Instant,
}

pub(crate) struct TrafficStore {
    path: PathBuf,
    inner: Mutex<Inner>,
    /// Ceiling that settings cannot exceed; the file may live in RAM.
    storage_ceiling: Option<u64>,
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
    #[cfg(test)]
    pub(crate) fn open(path: PathBuf) -> Result<Self, TrafficError> {
        Self::open_with_ceiling(path, None)
    }

    pub(crate) fn open_with_ceiling(
        path: PathBuf,
        storage_ceiling: Option<u64>,
    ) -> Result<Self, TrafficError> {
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
        let persisted_bytes = fs::metadata(&path).map_or(0, |metadata| {
            usize::try_from(metadata.len()).unwrap_or(usize::MAX)
        });
        Ok(Self {
            storage_ceiling,
            path,
            inner: Mutex::new(Inner {
                persisted_bytes,
                last_persist: Instant::now(),
                applied_migrations: document.applied_migrations,
                settings: capped(document.settings, storage_ceiling),
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
        Ok(summarize(&inner, since, None, dimension))
    }

    fn history_range(
        &self,
        since: i64,
        until: Option<i64>,
        dimension: TrafficDimension,
    ) -> Result<TrafficHistory, TrafficError> {
        let inner = self.lock()?;
        Ok(summarize(&inner, since, until, dimension))
    }

    pub(crate) fn update_settings(
        &self,
        settings: TrafficSettings,
        now: i64,
    ) -> Result<(), TrafficError> {
        traffic_rotation::validate(&settings)?;
        let mut inner = self.lock()?;
        inner.settings = capped(settings, self.storage_ceiling);
        rotate(&mut inner, now);
        inner.dirty = true;
        persist(&self.path, &mut inner, self.storage_ceiling)
    }

    pub(crate) fn clear(&self) -> Result<(), TrafficError> {
        let mut inner = self.lock()?;
        inner.records.clear();
        inner.dirty = true;
        persist(&self.path, &mut inner, self.storage_ceiling)
    }

    pub(crate) fn flush(&self) -> Result<(), TrafficError> {
        let mut inner = self.lock()?;
        if inner.dirty {
            persist(&self.path, &mut inner, self.storage_ceiling)?;
        }
        Ok(())
    }

    pub(crate) fn maintain(&self, now: i64) -> Result<(), TrafficError> {
        let mut inner = self.lock()?;
        if rotate(&mut inner, now) {
            inner.dirty = true;
        }
        if inner.dirty && inner.last_persist.elapsed() >= PERSIST_INTERVAL {
            persist(&self.path, &mut inner, self.storage_ceiling)?;
        }
        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, Inner>, TrafficError> {
        self.inner.lock().map_err(|_| TrafficError::Lock)
    }
}

fn summarize(
    inner: &Inner,
    since: i64,
    until: Option<i64>,
    dimension: TrafficDimension,
) -> TrafficHistory {
    let mut totals = HashMap::<String, Totals>::new();
    for (key, value) in &inner.records {
        if key.time < since
            || until.is_some_and(|until| key.time > until)
            || key.dimension != dimension
        {
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
        (right.download + right.upload)
            .cmp(&(left.download + left.upload))
            .then_with(|| left.label.cmp(&right.label))
    });
    TrafficHistory {
        settings: inner.settings.clone(),
        storage_bytes: inner.persisted_bytes,
        totals,
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

/// Keeps the reported limit honest on hosts whose storage has a fixed ceiling.
pub(crate) fn capped(mut settings: TrafficSettings, ceiling: Option<u64>) -> TrafficSettings {
    settings.max_bytes = match (settings.max_bytes, ceiling) {
        (Some(limit), Some(ceiling)) => Some(limit.min(ceiling)),
        (limit, ceiling) => limit.or(ceiling),
    };
    settings
}

fn persist(path: &Path, inner: &mut Inner, ceiling: Option<u64>) -> Result<(), TrafficError> {
    let limit = capped(inner.settings.clone(), ceiling).max_bytes;
    let mut data = encoded(inner)?;
    while let Some(limit) = limit.filter(|limit| data.len() as u64 > *limit) {
        if inner.records.is_empty() {
            break;
        }
        let times = inner.records.keys().map(|key| key.time);
        let cutoff = traffic_rotation::eviction_cutoff(times, data.len() as u64, limit);
        inner.records.retain(|key, _| key.time > cutoff);
        data = encoded(inner)?;
    }
    sempre_state::write_atomic(path, &data, 0o600).map_err(|source| TrafficError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    inner.dirty = false;
    inner.persisted_bytes = data.len();
    inner.last_persist = Instant::now();
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
        applied_migrations: inner.applied_migrations.clone(),
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
    until: Option<i64>,
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
    let history = if query.since == 0 && query.until.is_none() {
        state
            .traffic
            .history(0, query.dimension, Utc::now().timestamp_millis())
    } else {
        state
            .traffic
            .history_range(query.since, query.until, query.dimension)
    };
    result(history)
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
