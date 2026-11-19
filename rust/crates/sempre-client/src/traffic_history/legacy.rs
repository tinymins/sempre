use std::{
    collections::HashMap,
    fmt,
    fs::{self, File},
    io::{self, BufReader, Seek},
    path::Path,
};

use sempre_state::AppliedMigration;
use serde::{
    Deserialize,
    de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor},
};
use sha2::{Digest, Sha256};

use super::{
    Row, SETTINGS_FILE, TrafficDimension, TrafficError, TrafficSettings, capped,
    segments::{Segments, sync_directory},
    write_settings,
};
use crate::{traffic_history_migrations, traffic_rotation};

const IMPORT_BATCH: usize = 4096;

pub(super) fn prepare(
    root: &Path,
    source: &Path,
    ceiling: Option<u64>,
) -> Result<(), TrafficError> {
    let mut file = match File::open(source) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    file.try_lock().map_err(io::Error::from)?;
    let settings = metadata(&mut file)?;
    let staging = root.with_added_extension("sempre-import-v1");
    let parent = root
        .parent()
        .ok_or(TrafficError::Invalid("missing history parent directory"))?;
    remove_staging(&staging)?;
    if root.try_exists()? {
        // A crash can leave the published store alongside the original. Re-read
        // both before removing the source, even if their settings look identical.
        let mut original = Summary::default();
        each_row(&mut file, |row| {
            original.observe(&row);
            Ok(())
        })?;
        verify(root, &settings, original, ceiling)?;
    } else {
        fs::create_dir(&staging)?;
        let result = (|| {
            write_settings(&staging, &settings)?;
            let mut segments = Segments::open(&staging)?;
            let settings_len = fs::metadata(staging.join(SETTINGS_FILE))?.len();
            let limit = capped(settings.clone(), ceiling).max_bytes;
            let mut original = Summary::default();
            let mut batch = Vec::with_capacity(IMPORT_BATCH);
            each_row(&mut file, |row| {
                original.observe(&row);
                batch.push(row);
                if batch.len() == IMPORT_BATCH {
                    segments.append_import(&batch, settings_len, limit)?;
                    batch.clear();
                }
                Ok(())
            })?;
            if !batch.is_empty() {
                segments.append_import(&batch, settings_len, limit)?;
            }
            drop(segments);
            verify(&staging, &settings, original, ceiling)?;
            sync_directory(&staging)?;
            fs::rename(&staging, root)?;
            Ok::<(), TrafficError>(())
        })();
        if let Err(error) = result {
            remove_staging(&staging)?;
            return Err(error);
        }
    }
    // Persist the new root entry before releasing its only original source.
    sync_directory(parent)?;
    fs::remove_file(source)?;
    sync_directory(parent)?;
    Ok(())
}

// Only remove files the importer itself can create in its reserved sibling
// directory. Refuse foreign files, subdirectories and symlinks without touching them.
fn remove_staging(staging: &Path) -> Result<(), TrafficError> {
    match fs::symlink_metadata(staging) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Err(TrafficError::Invalid("unexpected traffic import workspace")),
        Err(error) => return Err(error.into()),
    }
    for entry in fs::read_dir(staging)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !entry.file_type()?.is_file()
            || !(is_segment(&name)
                || name == "cursor"
                || name == SETTINGS_FILE
                || name.starts_with(".sempre-write-"))
        {
            return Err(TrafficError::Invalid(
                "unrecognized file in traffic import workspace",
            ));
        }
    }
    fs::remove_dir_all(staging)?;
    Ok(())
}

fn is_segment(name: &str) -> bool {
    name.strip_suffix(".jsonl")
        .is_some_and(|id| id.len() == 16 && id.bytes().all(|c| c.is_ascii_hexdigit()))
}

fn verify(
    root: &Path,
    expected: &TrafficSettings,
    original: Summary,
    ceiling: Option<u64>,
) -> Result<(), TrafficError> {
    if !fs::symlink_metadata(root)?.is_dir() {
        return Err(TrafficError::Invalid("invalid published traffic directory"));
    }
    let settings = fs::read(root.join(SETTINGS_FILE))?;
    if serde_json::from_slice::<serde_json::Value>(&settings)? != serde_json::to_value(expected)? {
        return Err(TrafficError::Invalid("imported traffic settings differ"));
    }
    let segments = Segments::inspect(root)?;
    // Only a complete, published import can be recovered. Do not adopt an
    // unrelated directory, an orphan segment, or an uncommitted extra tail.
    let mut stored = 0;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !entry.file_type()?.is_file()
            || !(is_segment(&name) || name == "cursor" || name == SETTINGS_FILE)
        {
            return Err(TrafficError::Invalid(
                "unexpected file in imported traffic history",
            ));
        }
        let len = entry.metadata()?.len();
        if len == 0 && is_segment(&name) {
            return Err(TrafficError::Invalid(
                "empty segment in imported traffic history",
            ));
        }
        stored += len;
    }
    if stored != segments.storage_bytes() + settings.len() as u64 {
        return Err(TrafficError::Invalid(
            "uncommitted data in imported traffic history",
        ));
    }
    let required = segments.budget_bytes(settings.len() as u64);
    if let Some(limit) = capped(expected.clone(), ceiling).max_bytes
        && required > limit
    {
        return Err(TrafficError::ImportBudget { required, limit });
    }
    let mut actual = Summary::default();
    segments.visit(i64::MIN, i64::MAX, |row| actual.observe(&row))?;
    if !actual.matches(original) {
        return Err(TrafficError::Invalid(
            "imported traffic records differ from original JSON",
        ));
    }
    Ok(())
}

#[derive(Default)]
struct Summary {
    rows: u64,
    bounds: Option<(i64, i64)>,
    totals: HashMap<TrafficDimension, (i128, i128)>,
    digest: Sha256,
}

impl Summary {
    fn observe(&mut self, row: &Row) {
        let Row(time, dimension, label, download, upload) = row;
        self.rows += 1;
        let (first, last) = self.bounds.get_or_insert((*time, *time));
        *first = (*first).min(*time);
        *last = (*last).max(*time);
        let totals = self.totals.entry(*dimension).or_default();
        totals.0 += i128::from(*download);
        totals.1 += i128::from(*upload);
        self.digest.update(time.to_le_bytes());
        self.digest.update([*dimension as u8]);
        self.digest.update((label.len() as u64).to_le_bytes());
        self.digest.update(label.as_bytes());
        self.digest.update(download.to_le_bytes());
        self.digest.update(upload.to_le_bytes());
    }

    fn matches(self, other: Self) -> bool {
        self.rows == other.rows
            && self.bounds == other.bounds
            && self.totals == other.totals
            && self.digest.finalize() == other.digest.finalize()
    }
}

// Every published schema migration changes only metadata. Let the existing
// registry verify the original ledger, skipping the large row array on this pass.
fn metadata(file: &mut File) -> Result<TrafficSettings, TrafficError> {
    #[derive(Deserialize)]
    struct Header {
        #[serde(rename = "records")]
        _records: IgnoredAny,
        #[serde(flatten)]
        fields: serde_json::Map<String, serde_json::Value>,
    }
    #[derive(Deserialize)]
    struct Migrated {
        schema: u32,
        applied_migrations: Vec<AppliedMigration>,
        settings: TrafficSettings,
    }
    let header: Header = serde_json::from_reader(BufReader::new(file))?;
    let migration = traffic_history_migrations::run(&serde_json::to_vec(&header.fields)?)?;
    let header: Migrated = serde_json::from_value(migration.value)?;
    if header.schema != traffic_history_migrations::CURRENT_SCHEMA {
        return Err(TrafficError::Schema(header.schema));
    }
    traffic_history_migrations::validate_ledger(&header.applied_migrations)?;
    traffic_rotation::validate(&header.settings)?;
    Ok(header.settings)
}

#[derive(Deserialize)]
struct StoredRecord {
    time: i64,
    dimension: TrafficDimension,
    label: String,
    download: i64,
    upload: i64,
}

// A serde sequence visitor streams the original nested JSON array one row at
// a time. Keep filesystem errors from the callback in their original type.
fn each_row(
    file: &mut File,
    mut apply: impl FnMut(Row) -> Result<(), TrafficError>,
) -> Result<(), TrafficError> {
    file.rewind()?;
    let mut failure = None;
    let mut callback = |record: StoredRecord| {
        let row = Row(
            record.time,
            record.dimension,
            record.label,
            record.download,
            record.upload,
        );
        failure = apply(row).err();
        failure.is_none()
    };
    let mut decoder = serde_json::Deserializer::from_reader(BufReader::new(file));
    let result = decoder
        .deserialize_map(Rows(&mut callback))
        .and_then(|()| decoder.end());
    if let Some(error) = failure {
        return Err(error);
    }
    result.map_err(Into::into)
}

struct Rows<'a>(&'a mut dyn FnMut(StoredRecord) -> bool);

impl<'de> Visitor<'de> for Rows<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("legacy traffic document with a records array")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
        while let Some(key) = map.next_key::<String>()? {
            if key == "records" {
                map.next_value_seed(Rows(self.0))?;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(())
    }

    fn visit_seq<S: SeqAccess<'de>>(self, mut rows: S) -> Result<(), S::Error> {
        while let Some(record) = rows.next_element()? {
            if !(self.0)(record) {
                return Err(de::Error::custom("traffic import interrupted"));
            }
        }
        Ok(())
    }
}

impl<'de> DeserializeSeed<'de> for Rows<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}
