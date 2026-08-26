use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use flate2::read::GzDecoder;

use crate::{ArtifactError, Result};

pub const MAX_EXPANDED_SIZE: u64 = 2 << 30;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
    Gzip,
    Raw,
}

impl TryFrom<&str> for ArchiveFormat {
    type Error = ArtifactError;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "zip" => Ok(Self::Zip),
            "tar.gz" => Ok(Self::TarGz),
            "gz" => Ok(Self::Gzip),
            "raw" => Ok(Self::Raw),
            _ => Err(ArtifactError::invalid(format!(
                "unsupported archive format {value:?}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractOptions {
    pub format: ArchiveFormat,
    pub single_file_name: Option<String>,
}

pub fn extract(path: &Path, destination: &Path, options: &ExtractOptions) -> Result<()> {
    fs::create_dir_all(destination)
        .map_err(|error| ArtifactError::io("create extraction directory", error))?;
    secure_directory(destination)?;
    match options.format {
        ArchiveFormat::Zip => extract_zip(path, destination),
        ArchiveFormat::TarGz => extract_tar_gz(path, destination),
        ArchiveFormat::Gzip => extract_gzip(
            path,
            destination,
            single_file_name(options, "gzip")?,
            MAX_EXPANDED_SIZE,
        ),
        ArchiveFormat::Raw => extract_raw(path, destination, single_file_name(options, "raw")?),
    }
}

pub fn find(root: &Path, name: &str) -> Result<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| ArtifactError::io("scan extracted archive", error))?;
        for entry in entries {
            let entry = entry.map_err(|error| ArtifactError::io("scan archive entry", error))?;
            let file_type = entry
                .file_type()
                .map_err(|error| ArtifactError::io("inspect archive entry", error))?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(name)
            {
                return Ok(entry.path());
            }
        }
    }
    Err(ArtifactError::invalid(format!(
        "archive does not contain {name}"
    )))
}

fn extract_raw(path: &Path, destination: &Path, name: &str) -> Result<()> {
    let source = File::open(path).map_err(|error| ArtifactError::io("open raw artifact", error))?;
    let metadata = source
        .metadata()
        .map_err(|error| ArtifactError::io("inspect raw artifact", error))?;
    if !metadata.is_file() || metadata.len() > MAX_EXPANDED_SIZE {
        return Err(ArtifactError::invalid(format!(
            "raw artifact exceeds {MAX_EXPANDED_SIZE} bytes or is not a regular file"
        )));
    }
    let target = safe_target(destination, name)?;
    write_limited(&target, source, 0o600, MAX_EXPANDED_SIZE)?;
    Ok(())
}

fn extract_gzip(path: &Path, destination: &Path, name: &str, limit: u64) -> Result<()> {
    let source = File::open(path).map_err(|error| ArtifactError::io("open gzip", error))?;
    let reader = GzDecoder::new(source);
    let target = safe_target(destination, name)?;
    write_limited(&target, reader, 0o600, limit)?;
    Ok(())
}

fn extract_zip(path: &Path, destination: &Path) -> Result<()> {
    let source = File::open(path).map_err(|error| ArtifactError::io("open ZIP", error))?;
    let mut archive =
        zip::ZipArchive::new(source).map_err(|error| ArtifactError::zip("open ZIP", error))?;
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| ArtifactError::zip("read ZIP entry", error))?;
        expanded = expanded
            .checked_add(entry.size())
            .filter(|size| *size <= MAX_EXPANDED_SIZE)
            .ok_or_else(|| {
                ArtifactError::invalid(format!(
                    "ZIP archive expands beyond {MAX_EXPANDED_SIZE} bytes"
                ))
            })?;
        let target = safe_target(destination, entry.name())?;
        if entry.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|error| ArtifactError::io("create ZIP directory", error))?;
            secure_directory(&target)?;
            continue;
        }
        if !zip_entry_is_regular(&entry) {
            continue;
        }
        let mode = entry.unix_mode().unwrap_or(0o600);
        let size = entry.size();
        write_limited(&target, &mut entry, mode, size)?;
    }
    Ok(())
}

fn extract_tar_gz(path: &Path, destination: &Path) -> Result<()> {
    let source = File::open(path).map_err(|error| ArtifactError::io("open tar.gz", error))?;
    let decoder = GzDecoder::new(source);
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| ArtifactError::io("read tar", error))?;
    let mut expanded = 0_u64;
    for entry in entries {
        let mut entry = entry.map_err(|error| ArtifactError::io("read tar entry", error))?;
        let path = entry
            .path()
            .map_err(|error| ArtifactError::io("read tar entry path", error))?;
        let name = path
            .to_str()
            .ok_or_else(|| ArtifactError::invalid("tar entry path is not UTF-8"))?;
        let target = safe_target(destination, name)?;
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|error| ArtifactError::io("create tar directory", error))?;
            secure_directory(&target)?;
        } else if kind.is_file() {
            let size = entry.size();
            expanded = expanded
                .checked_add(size)
                .filter(|size| *size <= MAX_EXPANDED_SIZE)
                .ok_or_else(|| {
                    ArtifactError::invalid(format!(
                        "tar archive expands beyond {MAX_EXPANDED_SIZE} bytes"
                    ))
                })?;
            let mode = entry.header().mode().unwrap_or(0o600);
            write_limited(&target, &mut entry, mode, size)?;
        }
    }
    Ok(())
}

fn write_limited(path: &Path, source: impl Read, mode: u32, limit: u64) -> Result<u64> {
    #[cfg(not(unix))]
    let _ = mode;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| ArtifactError::io("create archive directory", error))?;
        secure_directory(parent)?;
    }
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode & 0o777);
    }
    let mut target = options
        .open(path)
        .map_err(|error| ArtifactError::io("create extracted file", error))?;
    let written = io::copy(&mut source.take(limit.saturating_add(1)), &mut target)
        .map_err(|error| ArtifactError::io("extract archive file", error))?;
    if written > limit {
        drop(target);
        let _ = fs::remove_file(path);
        return Err(ArtifactError::invalid(format!(
            "archive file expands beyond {limit} bytes"
        )));
    }
    target
        .flush()
        .map_err(|error| ArtifactError::io("flush extracted file", error))?;
    Ok(written)
}

fn safe_target(destination: &Path, name: &str) -> Result<PathBuf> {
    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/') {
        return Err(ArtifactError::invalid(format!(
            "archive entry is absolute: {name:?}"
        )));
    }
    let mut target = destination.to_path_buf();
    let mut found = false;
    for component in normalized.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                return Err(ArtifactError::invalid(format!(
                    "archive entry escapes extraction directory: {name:?}"
                )));
            }
            value if value.contains(':') => {
                return Err(ArtifactError::invalid(format!(
                    "archive entry has a platform path prefix: {name:?}"
                )));
            }
            value => {
                target.push(value);
                found = true;
            }
        }
    }
    if !found {
        return Err(ArtifactError::invalid(format!(
            "archive entry has an empty path: {name:?}"
        )));
    }
    Ok(target)
}

fn single_file_name<'a>(options: &'a ExtractOptions, format: &str) -> Result<&'a str> {
    let name = options.single_file_name.as_deref().unwrap_or_default();
    if name.is_empty() || name.contains(['/', '\\']) || matches!(name, "." | "..") {
        return Err(ArtifactError::invalid(format!(
            "single-file {format} output name must be a non-empty base name"
        )));
    }
    Ok(name)
}

fn zip_entry_is_regular<R: Read + ?Sized>(entry: &zip::read::ZipFile<'_, R>) -> bool {
    entry
        .unix_mode()
        .is_none_or(|mode| mode & 0o170_000 == 0 || mode & 0o170_000 == 0o100_000)
}

#[cfg(unix)]
fn secure_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| ArtifactError::io("secure extraction directory", error))
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn secure_directory(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_path_check_rejects_escape_and_platform_prefixes() {
        let root = Path::new("root");
        for name in ["../evil", "a/../../evil", r"..\evil", "/evil", "C:/evil"] {
            assert!(safe_target(root, name).is_err(), "{name}");
        }
        assert_eq!(
            safe_target(root, "core/bin").expect("target"),
            root.join("core/bin")
        );
    }

    #[test]
    fn gzip_limit_removes_partial_output() {
        use flate2::{Compression, write::GzEncoder};

        let root = tempfile::tempdir().expect("temporary directory");
        let source = root.path().join("source.gz");
        let mut encoder =
            GzEncoder::new(File::create(&source).expect("source"), Compression::fast());
        encoder.write_all(&[0_u8; 32]).expect("compressed data");
        encoder.finish().expect("finish gzip");
        let destination = root.path().join("out");
        fs::create_dir(&destination).expect("destination");
        assert!(extract_gzip(&source, &destination, "core", 16).is_err());
        assert!(!destination.join("core").exists());
    }
}
