use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use tokio::io::{AsyncRead, AsyncReadExt as _};

pub async fn copy_rolling(
    mut reader: impl AsyncRead + Unpin,
    path: PathBuf,
    limit: u64,
    backups: usize,
    observer: Option<crate::OutputObserver>,
    stream: &'static str,
) -> io::Result<()> {
    let mut writer = RollingWriter::open(&path, limit, backups)?;
    let mut buffer = vec![0_u8; 16 << 10];
    let mut pending = Vec::new();
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            if let Some(observer) = &observer
                && !pending.is_empty()
            {
                observer(stream, &String::from_utf8_lossy(&pending));
            }
            writer.flush()?;
            return Ok(());
        }
        writer.write_all(&buffer[..count])?;
        if let Some(observer) = &observer {
            pending.extend_from_slice(&buffer[..count]);
            while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                observer(stream, &String::from_utf8_lossy(&pending[..end]));
                pending.drain(..=end);
            }
            if pending.len() >= 16 << 10 {
                observer(stream, &String::from_utf8_lossy(&pending));
                pending.clear();
            }
        }
    }
}

pub fn append_rolling(path: &Path, content: &[u8], limit: u64, backups: usize) -> io::Result<()> {
    let mut writer = RollingWriter::open(path, limit, backups)?;
    writer.write_all(content)?;
    writer.flush()
}

struct RollingWriter {
    path: PathBuf,
    limit: u64,
    backups: usize,
    size: u64,
    file: File,
}

impl RollingWriter {
    fn open(path: &Path, limit: u64, backups: usize) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let size = fs::metadata(path).map_or(0, |metadata| metadata.len());
        let file = open_private(path)?;
        Ok(Self {
            path: path.into(),
            limit,
            backups,
            size,
            file,
        })
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        if self.backups > 0 {
            let oldest = backup(&self.path, self.backups);
            if oldest.exists() {
                fs::remove_file(oldest)?;
            }
            for index in (1..self.backups).rev() {
                let source = backup(&self.path, index);
                if source.exists() {
                    fs::rename(source, backup(&self.path, index + 1))?;
                }
            }
            if self.path.exists() {
                fs::rename(&self.path, backup(&self.path, 1))?;
            }
        } else if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        self.file = open_private(&self.path)?;
        self.size = 0;
        Ok(())
    }
}

impl io::Write for RollingWriter {
    fn write(&mut self, content: &[u8]) -> io::Result<usize> {
        if self.limit > 0 && self.size >= self.limit {
            self.rotate()?;
        }
        let allowed = if self.limit == 0 {
            content.len()
        } else {
            usize::try_from(self.limit.saturating_sub(self.size))
                .unwrap_or(usize::MAX)
                .min(content.len())
        };
        let count = self.file.write(&content[..allowed])?;
        self.size = self.size.saturating_add(count as u64);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn backup(path: &Path, index: usize) -> PathBuf {
    let mut value: OsString = path.as_os_str().into();
    value.push(format!(".{index}"));
    value.into()
}

#[cfg(unix)]
fn open_private(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt as _;

    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_private(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}
