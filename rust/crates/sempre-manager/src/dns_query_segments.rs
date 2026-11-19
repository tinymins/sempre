use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use sempre_dns::DnsQueryEvent;

const ROWS_PER_SEGMENT: u64 = 256;

struct Segment {
    path: PathBuf,
    start: u64,
    rows: u64,
    complete_bytes: u64,
}

pub(super) struct Segments {
    directory: PathBuf,
    files: VecDeque<Segment>,
    file: Option<File>,
    next: u64,
    floor: u64,
    limit: usize,
}

impl Segments {
    pub(super) fn open(
        legacy: &Path,
        limit: usize,
    ) -> io::Result<(Self, VecDeque<Arc<DnsQueryEvent>>)> {
        let directory = legacy.with_extension("jsonl.d");
        let floor = match fs::read_to_string(directory.join("floor")) {
            Ok(value) => value.parse().map_err(io::Error::other)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error),
        };
        let mut paths = Vec::new();
        match fs::read_dir(&directory) {
            Ok(entries) => {
                for entry in entries {
                    let path = entry?.path();
                    if path
                        .extension()
                        .is_some_and(|extension| extension == "jsonl")
                        && let Some(start) = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .and_then(|s| s.parse::<u64>().ok())
                        && path == segment_path(&directory, start)
                    {
                        paths.push((start, path));
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        paths.sort_unstable();
        let mut files = VecDeque::new();
        let mut events = VecDeque::new();
        let mut next = floor;
        for (start, path) in std::iter::once((0, legacy.to_owned())).chain(paths) {
            let file = match File::open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            let mut reader = BufReader::new(file);
            let mut rows = 0;
            let mut complete_bytes = 0;
            let mut line = Vec::new();
            loop {
                line.clear();
                let bytes = reader.read_until(b'\n', &mut line)?;
                if bytes == 0 || (path != legacy && line.last() != Some(&b'\n')) {
                    break;
                }
                complete_bytes += bytes as u64;
                if let Ok(event) = serde_json::from_slice(&line) {
                    if start + rows >= floor {
                        if events.len() == limit {
                            events.pop_front();
                        }
                        events.push_back(Arc::new(event));
                    }
                    rows += 1;
                }
            }
            next = next.max(start + rows);
            files.push_back(Segment {
                path,
                start,
                rows,
                complete_bytes,
            });
        }
        Ok((
            Self {
                directory,
                files,
                file: None,
                next,
                floor,
                limit,
            },
            events,
        ))
    }

    pub(super) fn append(&mut self, event: &DnsQueryEvent) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(event)?;
        bytes.push(b'\n');
        self.prune()?;
        let rotate = self
            .files
            .back()
            .is_none_or(|s| s.rows >= ROWS_PER_SEGMENT || s.path.parent() != Some(&self.directory));
        if rotate {
            fs::create_dir_all(&self.directory)?;
            let path = segment_path(&self.directory, self.next);
            let mut options = OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            self.file = Some(options.open(&path)?);
            self.files.push_back(Segment {
                path,
                start: self.next,
                rows: 0,
                complete_bytes: 0,
            });
        }
        let segment = self.files.back_mut().expect("active DNS query segment");
        if self.file.is_none() {
            let file = OpenOptions::new().append(true).open(&segment.path)?;
            // Resume at the last complete record after an interrupted or partial write.
            if file.metadata()?.len() != segment.complete_bytes {
                file.set_len(segment.complete_bytes)?;
            }
            self.file = Some(file);
        }
        if let Err(error) = self
            .file
            .as_mut()
            .expect("open DNS query segment")
            .write_all(&bytes)
        {
            self.file = None;
            return Err(error);
        }
        segment.complete_bytes += bytes.len() as u64;
        segment.rows += 1;
        self.next += 1;
        Ok(())
    }

    // Before increasing the retention limit, pin the old logical boundary. A
    // partially retained segment can otherwise reveal previously expired entries.
    pub(super) fn pin_retention(&mut self) -> io::Result<()> {
        self.save_floor(self.next.saturating_sub(self.limit as u64))
    }

    pub(super) fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }

    pub(super) fn clear(&mut self) -> io::Result<()> {
        self.save_floor(self.next)?;
        self.file = None;
        Ok(())
    }

    fn save_floor(&mut self, floor: u64) -> io::Result<()> {
        if floor > self.floor {
            sempre_state::write_atomic(
                &self.directory.join("floor"),
                floor.to_string().as_bytes(),
                0o600,
            )?;
            self.floor = floor;
        }
        Ok(())
    }

    pub(super) fn prune(&mut self) -> io::Result<()> {
        let cutoff = self.floor.max(self.next.saturating_sub(self.limit as u64));
        while let Some(front) = self.files.front()
            && front.start + front.rows <= cutoff
        {
            if self.files.len() == 1 {
                self.file = None;
            }
            match fs::remove_file(&front.path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            self.files.pop_front();
        }
        Ok(())
    }
}

fn segment_path(directory: &Path, start: u64) -> PathBuf {
    directory.join(format!("{start:020}.jsonl"))
}
