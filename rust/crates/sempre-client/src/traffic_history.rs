use std::{
    collections::HashMap,
    path::Path,
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

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    api,
    api::AppState,
    traffic_rotation::{self, RotationError, TrafficSettings},
};

mod segments;
use segments::Segments;
const SETTINGS_FILE: &str = "settings.json";
mod legacy;

const BUCKET_MILLIS: i64 = 60_000;
const PERSIST_INTERVAL: Duration = Duration::from_mins(1);

#[derive(Debug, Error)]
pub(crate) enum TrafficError {
    #[error("traffic history file: {0}")]
    Io(#[from] std::io::Error),
    #[error("traffic history task: {0}")]
    Task(#[from] tokio::task::JoinError),
    #[error("traffic history JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "retaining the complete traffic history needs {required} bytes, exceeding the {limit}-byte limit; the original JSON is intact"
    )]
    ImportBudget { required: u64, limit: u64 },
    #[error("traffic history lock is poisoned")]
    Lock,
    #[error("unsupported traffic history schema {0}")]
    Schema(u32),
    #[error("invalid traffic history: {0}")]
    Invalid(&'static str),
    #[error(transparent)]
    Migration(#[from] sempre_state::MigrationError),
    #[error(transparent)]
    Rotation(#[from] RotationError),
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialOrd, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TrafficDimension {
    Device,
    User,
    Host,
    Outbound,
    Process,
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct RecordKey {
    time: i64,
    dimension: TrafficDimension,
    label: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Totals {
    download: i64,
    upload: i64,
}

impl Totals {
    fn add(&mut self, other: Self) {
        self.download += other.download;
        self.upload += other.upload;
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct Row(i64, TrafficDimension, String, i64, i64);

struct Inner {
    segments: Segments,
    ceiling: Option<u64>,
    settings: TrafficSettings,
    settings_len: u64,
    pending: HashMap<RecordKey, Totals>,
    last_persist: Instant,
}

// Rename is the logical commit. Callers update their in-memory view before
// syncing the directory so an error after rename never replays committed data.
fn replace_metadata(root: &Path, name: &str, bytes: &[u8]) -> Result<(), TrafficError> {
    use std::io::Write;
    let mut temporary = tempfile::Builder::new()
        .prefix(".sempre-write-")
        .tempfile_in(root)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(root.join(name))
        .map_err(|error| error.error)?;
    Ok(())
}

fn write_settings(root: &Path, settings: &TrafficSettings) -> Result<(), TrafficError> {
    replace_metadata(root, SETTINGS_FILE, &serde_json::to_vec(settings)?)?;
    sempre_state::sync_directory(root)?;
    Ok(())
}

pub(crate) struct TrafficStore {
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
    #[cfg(test)]
    pub(crate) fn open(path: impl AsRef<Path>) -> Result<Self, TrafficError> {
        Self::open_with_ceiling(path, None)
    }

    pub(crate) fn open_with_ceiling(
        path: impl AsRef<Path>,
        storage_ceiling: Option<u64>,
    ) -> Result<Self, TrafficError> {
        let root = path.as_ref();
        legacy::prepare(root, &root.with_extension("json"), storage_ceiling)?;
        let segments = Segments::open(root)?;
        let bytes = match std::fs::read(root.join(SETTINGS_FILE)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let settings = TrafficSettings::default();
                write_settings(root, &settings)?;
                serde_json::to_vec(&settings)?
            }
            Err(error) => return Err(error.into()),
        };
        let settings = serde_json::from_slice(&bytes)?;
        traffic_rotation::validate(&settings)?;
        Ok(Self {
            inner: Mutex::new(Inner {
                segments,
                ceiling: storage_ceiling,
                settings: capped(settings, storage_ceiling),
                settings_len: bytes.len() as u64,
                pending: HashMap::new(),
                last_persist: Instant::now(),
            }),
        })
    }

    pub(crate) fn record(&self, time: i64, deltas: Vec<TrafficDelta>) -> Result<(), TrafficError> {
        if deltas.is_empty() {
            return Ok(());
        }
        let mut inner = self.lock()?;
        let time = time - time.rem_euclid(BUCKET_MILLIS);
        for delta in deltas {
            let key = RecordKey {
                time,
                dimension: delta.dimension,
                label: delta.label,
            };
            let entry = inner.pending.entry(key).or_default();
            entry.add(Totals {
                download: delta.download,
                upload: delta.upload,
            });
        }
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
        summarize(&inner, since, None, dimension, now)
    }

    pub(crate) fn history_range(
        &self,
        since: i64,
        until: Option<i64>,
        dimension: TrafficDimension,
    ) -> Result<TrafficHistory, TrafficError> {
        summarize(
            &*self.lock()?,
            since,
            until,
            dimension,
            Utc::now().timestamp_millis(),
        )
    }

    pub(crate) fn update_settings(
        &self,
        settings: TrafficSettings,
        now: i64,
    ) -> Result<(), TrafficError> {
        traffic_rotation::validate(&settings)?;
        let mut inner = self.lock()?;
        // Commit the old policy's cutoff before a wider retention policy
        // can make the expired portion of a shared segment visible again.
        let cutoff = traffic_rotation::storage_cutoff(&inner.settings, now).unwrap_or(i64::MIN);
        let settings_len = inner.settings_len;
        inner.segments.persist(&[], cutoff, None, settings_len)?;
        inner.pending.retain(|key, _| key.time >= cutoff);
        let settings = capped(settings, inner.ceiling);
        let data = serde_json::to_vec(&settings)?;
        replace_metadata(inner.segments.root(), SETTINGS_FILE, &data)?;
        inner.settings = settings;
        inner.settings_len = data.len() as u64;
        inner.segments.settings_replaced()?;
        persist(&mut inner, now)
    }

    pub(crate) fn clear(&self) -> Result<(), TrafficError> {
        let mut inner = self.lock()?;
        let result = inner.segments.clear();
        if inner.segments.empty() {
            // Cursor publication precedes unlink. A cleanup error must not
            // leave older pending samples visible after a committed clear.
            inner.pending.clear();
        }
        result
    }

    pub(crate) fn flush(&self) -> Result<(), TrafficError> {
        let mut inner = self.lock()?;
        persist(&mut inner, Utc::now().timestamp_millis())
    }

    pub(crate) fn maintain(&self, now: i64) -> Result<(), TrafficError> {
        let mut inner = self.lock()?;
        if inner.last_persist.elapsed() >= PERSIST_INTERVAL {
            persist(&mut inner, now)?;
        }
        Ok(())
    }

    pub(crate) async fn blocking<T: Send + 'static>(
        self: &Arc<Self>,
        operation: impl FnOnce(&Self) -> Result<T, TrafficError> + Send + 'static,
    ) -> Result<T, TrafficError> {
        let store = Arc::clone(self);
        tokio::task::spawn_blocking(move || operation(&store)).await?
    }

    fn lock(&self) -> Result<MutexGuard<'_, Inner>, TrafficError> {
        self.inner.lock().map_err(|_| TrafficError::Lock)
    }
}

fn persist(inner: &mut Inner, now: i64) -> Result<(), TrafficError> {
    let cutoff = traffic_rotation::storage_cutoff(&inner.settings, now).unwrap_or(i64::MIN);
    let cutoff = inner.segments.visible_since(cutoff);
    let mut rows: Vec<_> = inner
        .pending
        .iter()
        .filter(|(key, _)| key.time >= cutoff)
        .map(|(key, value)| {
            Row(
                key.time,
                key.dimension,
                key.label.clone(),
                value.download,
                value.upload,
            )
        })
        .collect();
    rows.sort_unstable_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));
    inner
        .segments
        .persist(&rows, cutoff, inner.settings.max_bytes, inner.settings_len)?;
    inner.pending.clear();
    inner.last_persist = Instant::now();
    inner.segments.sync_publication()
}

fn summarize(
    inner: &Inner,
    since: i64,
    until: Option<i64>,
    dimension: TrafficDimension,
    now: i64,
) -> Result<TrafficHistory, TrafficError> {
    let since = traffic_rotation::storage_cutoff(&inner.settings, now)
        .map_or(since, |cutoff| since.max(cutoff));
    let since = inner.segments.visible_since(since);
    let until = until.unwrap_or(i64::MAX);
    let mut totals = HashMap::<String, Totals>::new();
    inner.segments.visit(since, until, |row| {
        if row.1 == dimension {
            totals.entry(row.2).or_default().add(Totals {
                download: row.3,
                upload: row.4,
            });
        }
    })?;
    for (key, delta) in &inner.pending {
        if (since..=until).contains(&key.time) && key.dimension == dimension {
            totals.entry(key.label.clone()).or_default().add(*delta);
        }
    }
    let mut totals = totals
        .into_iter()
        .map(|(label, totals)| TrafficTotal {
            label,
            download: totals.download,
            upload: totals.upload,
        })
        .collect::<Vec<_>>();
    totals.sort_by(|left, right| {
        (right.download + right.upload)
            .cmp(&(left.download + left.upload))
            .then_with(|| left.label.cmp(&right.label))
    });
    Ok(TrafficHistory {
        settings: inner.settings.clone(),
        storage_bytes: usize::try_from(inner.segments.storage_bytes() + inner.settings_len)
            .unwrap_or(usize::MAX),
        totals,
    })
}

/// Keeps the reported limit honest on hosts whose storage has a fixed ceiling.
pub(crate) fn capped(mut settings: TrafficSettings, ceiling: Option<u64>) -> TrafficSettings {
    settings.max_bytes = match (settings.max_bytes, ceiling) {
        (Some(limit), Some(ceiling)) => Some(limit.min(ceiling)),
        (limit, ceiling) => limit.or(ceiling),
    };
    settings
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
    let history = state
        .traffic
        .blocking(move |store| {
            if query.since == 0 && query.until.is_none() {
                store.history(0, query.dimension, Utc::now().timestamp_millis())
            } else {
                store.history_range(query.since, query.until, query.dimension)
            }
        })
        .await;
    result(history)
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<TrafficSettings>,
) -> Response {
    match state
        .traffic
        .blocking(move |store| store.update_settings(settings, Utc::now().timestamp_millis()))
        .await
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
    match state.traffic.blocking(TrafficStore::clear).await {
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
