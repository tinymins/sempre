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
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{api, api::AppState};

const SCHEMA: u32 = 1;
const BUCKET_MILLIS: i64 = 60_000;
const DEFAULT_RETENTION_HOURS: u32 = 24;
const DEFAULT_MAX_BYTES: u64 = 32 * 1024 * 1024;
const MIN_RETENTION_HOURS: u32 = 1;
const MAX_RETENTION_HOURS: u32 = 24 * 30;
const MIN_MAX_BYTES: u64 = 1024 * 1024;
const MAX_MAX_BYTES: u64 = 256 * 1024 * 1024;

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
    #[error("retention_hours must be between {MIN_RETENTION_HOURS} and {MAX_RETENTION_HOURS}")]
    Retention,
    #[error("max_bytes must be between {MIN_MAX_BYTES} and {MAX_MAX_BYTES}")]
    MaximumSize,
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
pub(crate) struct TrafficSettings {
    pub retention_hours: u32,
    pub max_bytes: u64,
}

impl Default for TrafficSettings {
    fn default() -> Self {
        Self {
            retention_hours: DEFAULT_RETENTION_HOURS,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
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
        let document = match fs::read(&path) {
            Ok(data) => serde_json::from_slice::<Document>(&data).map_err(|source| {
                TrafficError::Decode {
                    path: path.clone(),
                    source,
                }
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Document {
                schema: SCHEMA,
                settings: TrafficSettings::default(),
                records: Vec::new(),
            },
            Err(source) => {
                return Err(TrafficError::Read {
                    path: path.clone(),
                    source,
                });
            }
        };
        if document.schema != SCHEMA {
            return Err(TrafficError::Schema(document.schema));
        }
        validate_settings(&document.settings)?;
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
        rotate_by_age(&mut inner, time);
        inner.dirty = true;
        Ok(())
    }

    pub(crate) fn history(
        &self,
        since: i64,
        dimension: TrafficDimension,
    ) -> Result<TrafficHistory, TrafficError> {
        let inner = self.lock()?;
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
        validate_settings(&settings)?;
        let mut inner = self.lock()?;
        inner.settings = settings;
        rotate_by_age(&mut inner, now);
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

    fn lock(&self) -> Result<MutexGuard<'_, Inner>, TrafficError> {
        self.inner.lock().map_err(|_| TrafficError::Lock)
    }
}

fn validate_settings(settings: &TrafficSettings) -> Result<(), TrafficError> {
    if !(MIN_RETENTION_HOURS..=MAX_RETENTION_HOURS).contains(&settings.retention_hours) {
        return Err(TrafficError::Retention);
    }
    if !(MIN_MAX_BYTES..=MAX_MAX_BYTES).contains(&settings.max_bytes) {
        return Err(TrafficError::MaximumSize);
    }
    Ok(())
}

fn rotate_by_age(inner: &mut Inner, now: i64) {
    let cutoff = now - i64::from(inner.settings.retention_hours) * 3_600_000;
    inner.records.retain(|key, _| key.time >= cutoff);
}

fn persist(path: &Path, inner: &mut Inner) -> Result<(), TrafficError> {
    let mut data = encoded(inner)?;
    while data.len() as u64 > inner.settings.max_bytes && !inner.records.is_empty() {
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
        schema: SCHEMA,
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
    result(state.traffic.history(query.since, query.dimension))
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
        Err(TrafficError::Retention | TrafficError::MaximumSize) => api::api_error(
            StatusCode::BAD_REQUEST,
            "INVALID_TRAFFIC_SETTINGS",
            "traffic history rotation settings are outside the supported range",
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
mod tests {
    use super::*;

    #[test]
    fn history_is_bucketed_persisted_and_rotated_by_age() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("traffic.json");
        let store = TrafficStore::open(path.clone()).expect("store");
        store
            .record(
                3_540_000,
                vec![TrafficDelta {
                    dimension: TrafficDimension::Host,
                    label: "old.example".into(),
                    download: 10,
                    upload: 2,
                }],
            )
            .expect("old record");
        store
            .record(
                7_200_000,
                vec![TrafficDelta {
                    dimension: TrafficDimension::Host,
                    label: "new.example".into(),
                    download: 20,
                    upload: 4,
                }],
            )
            .expect("new record");
        store
            .update_settings(
                TrafficSettings {
                    retention_hours: 1,
                    max_bytes: MIN_MAX_BYTES,
                },
                7_200_000,
            )
            .expect("settings");
        let reopened = TrafficStore::open(path).expect("reopened store");
        let history = reopened
            .history(0, TrafficDimension::Host)
            .expect("history");
        assert_eq!(history.totals.len(), 1);
        assert_eq!(history.totals[0].label, "new.example");
    }

    #[test]
    fn maximum_size_rotation_drops_oldest_buckets_first() {
        let root = tempfile::tempdir().expect("temporary directory");
        let store = TrafficStore::open(root.path().join("traffic.json")).expect("store");
        for minute in 0..20 {
            store
                .record(
                    minute * BUCKET_MILLIS,
                    vec![TrafficDelta {
                        dimension: TrafficDimension::Host,
                        label: format!("{minute}-{}", "x".repeat(70_000)),
                        download: 1,
                        upload: 1,
                    }],
                )
                .expect("record");
        }
        store
            .update_settings(
                TrafficSettings {
                    retention_hours: MAX_RETENTION_HOURS,
                    max_bytes: MIN_MAX_BYTES,
                },
                20 * BUCKET_MILLIS,
            )
            .expect("settings");
        let history = store.history(0, TrafficDimension::Host).expect("history");
        assert!(u64::try_from(history.storage_bytes).expect("storage size") <= MIN_MAX_BYTES);
        assert!(history.totals.len() < 20);
        assert!(
            history
                .totals
                .iter()
                .all(|item| !item.label.starts_with("0-"))
        );
    }
}
