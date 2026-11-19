use std::{
    collections::{BTreeSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};

use sempre_state::sync_directory;
use serde::{Deserialize, Serialize};

use super::{Row, TrafficError};

const SEGMENT_BYTES: u64 = 128 * 1024;
const BLOCK: u64 = 4096;
const DAY: i64 = 86_400_000;
const MAX_CURSOR_BYTES: u64 = 96;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Cursor(u64, u64, u64, i64);

impl Default for Cursor {
    fn default() -> Self {
        Self(0, 0, 0, i64::MIN)
    }
}

#[derive(Clone)]
struct Segment {
    id: u64,
    len: u64,
    from: i64,
    until: i64,
    minutes: BTreeSet<i64>,
}

struct Block {
    segment: Segment,
    bytes: Vec<u8>,
}

pub(super) struct Segments {
    root: PathBuf,
    granularity: u64,
    files: VecDeque<Segment>,
    cursor: Cursor,
    cursor_len: u64,
    needs_cleanup: bool,
    unsynced: bool,
}

fn path(root: &Path, id: u64) -> PathBuf {
    root.join(format!("{id:016x}.jsonl"))
}

fn charged(size: u64, unit: u64) -> u64 {
    size.div_ceil(unit) * unit
}

fn metadata_budget(settings: u64, files: usize, unit: u64) -> u64 {
    // Settings, cursor, a replacement file and the segment directory entries.
    charged(settings, unit)
        + unit
        + charged(settings.max(MAX_CURSOR_BYTES), unit)
        + charged((files as u64 + 3) * 128, unit).max(unit)
}

impl Segments {
    pub(super) fn open(root: &Path) -> Result<Self, TrafficError> {
        fs::create_dir_all(root)?;
        let mut store = Self::inspect(root)?;
        store.cleanup()?;
        Ok(store)
    }

    pub(super) fn inspect(root: &Path) -> Result<Self, TrafficError> {
        let (cursor, cursor_len) = match fs::read(root.join("cursor")) {
            Ok(data) => (serde_json::from_slice(&data)?, data.len() as u64),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Cursor::default(), 0),
            Err(e) => return Err(e.into()),
        };
        if (cursor.0 == 0 && (cursor.1 != 0 || cursor.2 != 0))
            || (cursor.0 != 0 && (cursor.0 > cursor.1 || cursor.2 == 0))
        {
            return Err(TrafficError::Invalid("invalid segment cursor"));
        }
        let mut store = Self {
            root: root.to_owned(),
            granularity: fs2::allocation_granularity(root)?.max(1),
            files: VecDeque::new(),
            cursor,
            cursor_len,
            needs_cleanup: true,
            unsynced: false,
        };
        if cursor.0 != 0 {
            for id in cursor.0..=cursor.1 {
                let filename = path(root, id);
                let actual = fs::metadata(&filename)?.len();
                let len = if id == cursor.1 { cursor.2 } else { actual };
                if actual < len {
                    return Err(TrafficError::Invalid(
                        "committed traffic segment is truncated",
                    ));
                }
                let mut segment = Segment {
                    id,
                    len,
                    from: i64::MAX,
                    until: i64::MIN,
                    minutes: BTreeSet::new(),
                };
                read_rows(&filename, len, |row| {
                    segment.minutes.insert(row.0);
                    segment.from = segment.from.min(row.0);
                    segment.until = segment.until.max(row.0);
                })?;
                store.files.push_back(segment);
            }
        }
        Ok(store)
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn storage_bytes(&self) -> u64 {
        self.files.iter().map(|s| s.len).sum::<u64>() + self.cursor_len
    }

    pub(super) fn budget_bytes(&self, settings: u64) -> u64 {
        let unit = self.granularity;
        self.files.iter().map(|s| charged(s.len, unit)).sum::<u64>()
            + metadata_budget(settings, self.files.len(), unit)
    }

    pub(super) fn visit(
        &self,
        since: i64,
        until: i64,
        mut visitor: impl FnMut(Row),
    ) -> Result<(), TrafficError> {
        let since = since.max(self.cursor.3);
        for s in &self.files {
            if s.until >= since && s.from <= until {
                read_rows(&path(&self.root, s.id), s.len, |row| {
                    if (since..=until).contains(&row.0) {
                        visitor(row);
                    }
                })?;
            }
        }
        Ok(())
    }

    pub(super) fn empty(&self) -> bool {
        self.files.is_empty()
    }

    pub(super) fn clear(&mut self) -> Result<(), TrafficError> {
        self.cleanup()?;
        self.retire(self.files.len(), i64::MIN)
    }

    fn cleanup(&mut self) -> Result<(), TrafficError> {
        self.sync_publication()?;
        if !self.needs_cleanup {
            return Ok(());
        }
        let Cursor(first, last, committed, _) = self.cursor;
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(".sempre-write-") {
                fs::remove_file(entry.path())?;
                continue;
            }
            let Some(id) = name
                .strip_suffix(".jsonl")
                .filter(|v| v.len() == 16)
                .and_then(|v| u64::from_str_radix(v, 16).ok())
            else {
                continue;
            };
            if first == 0 || id < first || id > last {
                fs::remove_file(entry.path())?;
            } else if id == last {
                match entry.metadata()?.len().cmp(&committed) {
                    std::cmp::Ordering::Less => {
                        return Err(TrafficError::Invalid(
                            "committed traffic segment is truncated",
                        ));
                    }
                    std::cmp::Ordering::Greater => {
                        let file = OpenOptions::new().write(true).open(entry.path())?;
                        file.set_len(committed)?;
                        file.sync_all()?;
                    }
                    std::cmp::Ordering::Equal => (),
                }
            }
        }
        sync_directory(&self.root)?;
        self.needs_cleanup = false;
        Ok(())
    }

    fn publish(&mut self, next: Cursor) -> Result<(), TrafficError> {
        let data = serde_json::to_vec(&next)?;
        super::replace_metadata(&self.root, "cursor", &data)?;
        self.cursor = next;
        self.cursor_len = data.len() as u64;
        self.unsynced = true;
        Ok(())
    }

    pub(super) fn sync_publication(&mut self) -> Result<(), TrafficError> {
        if self.unsynced {
            sync_directory(&self.root)?;
            self.unsynced = false;
        }
        Ok(())
    }

    pub(super) fn settings_replaced(&mut self) -> Result<(), TrafficError> {
        self.unsynced = true;
        self.sync_publication()
    }

    pub(super) fn visible_since(&self, time: i64) -> i64 {
        time.max(self.cursor.3)
    }

    fn retire(&mut self, count: usize, floor: i64) -> Result<(), TrafficError> {
        let next = self
            .files
            .get(count)
            .map_or(Cursor(0, 0, 0, floor), |first| {
                Cursor(first.id, self.cursor.1, self.cursor.2, floor)
            });
        if self.cursor == next {
            return Ok(());
        }
        self.publish(next)?;
        if count == 0 {
            return self.sync_publication();
        }
        let removed = self.files.drain(..count).collect::<Vec<_>>();
        self.needs_cleanup = true;
        self.sync_publication()?;
        for s in removed {
            fs::remove_file(path(&self.root, s.id))?;
        }
        sync_directory(&self.root)?;
        self.needs_cleanup = false;
        Ok(())
    }

    pub(super) fn append_import(
        &mut self,
        rows: &[Row],
        settings: u64,
        limit: Option<u64>,
    ) -> Result<(), TrafficError> {
        self.cleanup()?;
        let unit = self.granularity;
        let target = segment_target(limit);
        let tail = self.files.back();
        let blocks = pack(rows, tail, self.cursor.1 + 1, target)?;
        let extends = blocks
            .front()
            .is_some_and(|b| Some(b.segment.id) == tail.map(|s| s.id));
        let old_count = self.files.len() - usize::from(extends);
        let required = self
            .files
            .iter()
            .take(old_count)
            .map(|s| charged(s.len, unit))
            .sum::<u64>()
            + blocks
                .iter()
                .map(|b| charged(b.segment.len, unit))
                .sum::<u64>()
            + metadata_budget(settings, old_count + blocks.len(), unit);
        if let Some(limit) = limit
            && required > limit
        {
            return Err(TrafficError::ImportBudget { required, limit });
        }
        self.write_blocks(blocks)?;
        self.sync_publication()
    }

    pub(super) fn persist(
        &mut self,
        rows: &[Row],
        cutoff: i64,
        limit: Option<u64>,
        settings: u64,
    ) -> Result<(), TrafficError> {
        self.cleanup()?;
        let unit = self.granularity;
        let cutoff = self.visible_since(cutoff);
        let expired = self.files.iter().take_while(|s| s.until < cutoff).count();
        let target = segment_target(limit);
        let next = self.cursor.1 + 1;
        let tail = self.files.back().filter(|_| expired < self.files.len());
        let mut blocks = pack(rows, tail, next, target)?;
        let mut remove = expired;
        let mut quota_floor = self.cursor.3;
        if let Some(limit) = limit {
            let extends = blocks
                .front()
                .is_some_and(|b| Some(b.segment.id) == tail.map(|s| s.id));
            let old_count = self.files.len() - usize::from(extends);
            let mut size = self
                .files
                .iter()
                .take(old_count)
                .skip(remove)
                .map(|s| charged(s.len, unit))
                .sum::<u64>()
                + blocks
                    .iter()
                    .map(|b| charged(b.segment.len, unit))
                    .sum::<u64>();
            while remove < old_count
                && size + metadata_budget(settings, old_count - remove + blocks.len(), unit) > limit
            {
                size -= charged(self.files[remove].len, unit);
                remove += 1;
            }
            if size + metadata_budget(settings, old_count - remove + blocks.len(), unit) > limit {
                remove = self.files.len();
                if extends {
                    blocks = pack(rows, None, next, target)?;
                }
                size = blocks.iter().map(|b| charged(b.segment.len, unit)).sum();
                while size + metadata_budget(settings, blocks.len(), unit) > limit {
                    let Some(old) = blocks.pop_front() else {
                        return Err(TrafficError::Invalid(
                            "traffic storage limit below metadata size",
                        ));
                    };
                    quota_floor = quota_floor.max(old.segment.until.saturating_add(1));
                    size -= charged(old.segment.len, unit);
                }
            }
        }
        // A minute can straddle segment boundaries. Once capacity evicts
        // any part of it, hide the whole minute, including new rows from an
        // oversized batch. Queries must never report partial minute totals.
        for s in self.files.iter().take(remove).skip(expired) {
            quota_floor = quota_floor.max(s.until.saturating_add(1));
        }
        let floor = self
            .files
            .iter()
            .skip(remove)
            .filter(|s| s.from < cutoff)
            .filter_map(|s| s.minutes.range(self.cursor.3..cutoff).next_back().copied())
            .max()
            .map_or(quota_floor, |time| quota_floor.max(time.saturating_add(1)));
        if remove == self.files.len() {
            while blocks.front().is_some_and(|b| b.segment.until < floor) {
                blocks.pop_front();
            }
        }
        // Commit and unlink the oldest prefix before allocating new bytes.
        self.retire(remove, floor)?;
        self.write_blocks(blocks)
    }

    fn write_blocks(&mut self, blocks: VecDeque<Block>) -> Result<(), TrafficError> {
        if blocks.is_empty() {
            return Ok(());
        }
        self.needs_cleanup = true;
        let first = self.files.front().map_or(blocks[0].segment.id, |s| s.id);
        let mut created = false;
        for block in &blocks {
            let existing = self.files.back().is_some_and(|s| s.id == block.segment.id);
            let mut options = OpenOptions::new();
            options.write(true).append(true).create_new(!existing);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(path(&self.root, block.segment.id))?;
            file.write_all(&block.bytes)?;
            file.sync_all()?;
            created |= !existing;
        }
        if created {
            sync_directory(&self.root)?;
        }
        let last = &blocks.back().expect("nonempty batch").segment;
        self.publish(Cursor(first, last.id, last.len, self.cursor.3))?;
        for b in blocks {
            if self.files.back().is_some_and(|s| s.id == b.segment.id) {
                self.files.pop_back();
            }
            self.files.push_back(b.segment);
        }
        self.needs_cleanup = false;
        Ok(())
    }
}

fn segment_target(limit: Option<u64>) -> u64 {
    limit.map_or(SEGMENT_BYTES, |n| (n / 32).clamp(BLOCK, SEGMENT_BYTES))
}

fn pack(
    rows: &[Row],
    tail: Option<&Segment>,
    mut next: u64,
    target: u64,
) -> Result<VecDeque<Block>, TrafficError> {
    let mut result = VecDeque::<Block>::new();
    let mut line = Vec::new();
    for row in rows {
        line.clear();
        serde_json::to_writer(&mut line, row)?;
        line.push(b'\n');
        let len = line.len() as u64;
        let active = result.back().map(|b| &b.segment).or(tail);
        let fits = active.is_some_and(|s| {
            s.len + len <= target && s.from.div_euclid(DAY) == row.0.div_euclid(DAY)
        });
        if result.is_empty() || !fits {
            let segment = if fits {
                tail.expect("existing active segment").clone()
            } else {
                let id = next;
                next += 1;
                Segment {
                    id,
                    len: 0,
                    from: row.0,
                    until: row.0,
                    minutes: BTreeSet::new(),
                }
            };
            result.push_back(Block {
                segment,
                bytes: Vec::new(),
            });
        }
        let b = result.back_mut().expect("active segment");
        b.segment.minutes.insert(row.0);
        b.segment.len += len;
        b.segment.from = b.segment.from.min(row.0);
        b.segment.until = b.segment.until.max(row.0);
        b.bytes.extend_from_slice(&line);
    }
    Ok(result)
}

fn read_rows(file: &Path, len: u64, mut visitor: impl FnMut(Row)) -> Result<(), TrafficError> {
    let mut input = BufReader::new(File::open(file)?.take(len));
    let mut line = Vec::new();
    while input.read_until(b'\n', &mut line)? != 0 {
        if line.last() != Some(&b'\n') {
            return Err(TrafficError::Invalid("incomplete committed traffic row"));
        }
        visitor(serde_json::from_slice(&line)?);
        line.clear();
    }
    Ok(())
}
