use crate::{BundleError, release::ReleaseTarget};
use flate2::{Compression, write::GzEncoder};
use std::{
    fs::{self, File},
    io,
    path::Path,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

pub(super) fn zip_directory(
    destination: &Path,
    source: &Path,
    prefix: &str,
) -> Result<(), BundleError> {
    zip_directory_with_target(destination, source, prefix, None)
}

pub(super) fn zip_directory_with_target(
    destination: &Path,
    source: &Path,
    prefix: &str,
    target: Option<&ReleaseTarget>,
) -> Result<(), BundleError> {
    let file = File::create(destination).map_err(|source_error| BundleError::Io {
        operation: "create snapshot archive",
        path: destination.to_path_buf(),
        source: source_error,
    })?;
    let mut archive = ZipWriter::new(file);
    append_directory(&mut archive, source, source, prefix, target)?;
    archive.finish()?;
    Ok(())
}

fn append_directory(
    archive: &mut ZipWriter<File>,
    root: &Path,
    directory: &Path,
    prefix: &str,
    target: Option<&ReleaseTarget>,
) -> Result<(), BundleError> {
    let mut entries = fs::read_dir(directory)
        .map_err(|source_error| BundleError::Io {
            operation: "read snapshot archive directory",
            path: directory.to_path_buf(),
            source: source_error,
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source_error| BundleError::Io {
            operation: "read snapshot archive entry",
            path: directory.to_path_buf(),
            source: source_error,
        })?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = entry.file_type().map_err(|source_error| BundleError::Io {
            operation: "inspect snapshot archive entry",
            path: path.clone(),
            source: source_error,
        })?;
        if metadata.is_symlink() {
            return Err(BundleError::SymbolicLink(path));
        }
        let relative = path
            .strip_prefix(root)
            .expect("entry is below archive root");
        let name = format!("{prefix}/{}", relative.to_string_lossy().replace('\\', "/"));
        if metadata.is_dir() {
            archive.add_directory(
                format!("{name}/"),
                SimpleFileOptions::default().unix_permissions(0o700),
            )?;
            append_directory(archive, root, &path, prefix, target)?;
        } else if metadata.is_file() {
            let mode = match target.and_then(|target| release_mode(relative, target)) {
                Some(mode) => mode,
                None => archive_mode(&path)?,
            };
            archive.start_file(
                name,
                SimpleFileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .unix_permissions(mode),
            )?;
            let mut input = File::open(&path).map_err(|source_error| BundleError::Io {
                operation: "open snapshot archive file",
                path: path.clone(),
                source: source_error,
            })?;
            io::copy(&mut input, archive).map_err(|source_error| BundleError::Io {
                operation: "write snapshot archive file",
                path,
                source: source_error,
            })?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn archive_mode(path: &Path) -> Result<u32, BundleError> {
    use std::os::unix::fs::PermissionsExt as _;
    let metadata = path.metadata().map_err(|source| BundleError::Io {
        operation: "read snapshot archive permissions",
        path: path.to_path_buf(),
        source,
    })?;
    Ok(metadata.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn archive_mode(path: &Path) -> Result<u32, BundleError> {
    let executable = path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("exe")
            || extension.eq_ignore_ascii_case("cmd")
            || extension.eq_ignore_ascii_case("bat")
    });
    Ok(if executable { 0o755 } else { 0o600 })
}

fn release_mode(relative: &Path, target: &ReleaseTarget) -> Option<u32> {
    let name = relative.to_str().unwrap_or_default();
    let installer = match target.os.as_str() {
        "windows" => name == "install.cmd",
        "darwin" => matches!(name, "install.command" | "install.sh"),
        _ => matches!(name, "install.sh" | "install.desktop"),
    };
    (name == target.executable_name() || installer).then_some(0o755)
}

pub(super) fn tar_gz_directory(
    destination: &Path,
    source: &Path,
    prefix: &str,
    target: &ReleaseTarget,
) -> Result<(), BundleError> {
    let result = (|| -> io::Result<()> {
        let encoder = GzEncoder::new(File::create(destination)?, Compression::default());
        let mut archive = tar::Builder::new(encoder);
        append_tar_directory(&mut archive, source, source, Path::new(prefix), target)?;
        archive.into_inner()?.finish()?;
        Ok(())
    })();
    result.map_err(|source_error| BundleError::Io {
        operation: "archive release tar.gz",
        path: destination.to_path_buf(),
        source: source_error,
    })
}

fn append_tar_directory(
    archive: &mut tar::Builder<GzEncoder<File>>,
    root: &Path,
    directory: &Path,
    prefix: &Path,
    target: &ReleaseTarget,
) -> io::Result<()> {
    let relative = directory
        .strip_prefix(root)
        .expect("directory below archive root");
    archive.append_dir(prefix.join(relative), directory)?;
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(io::Error::other(format!(
                "symbolic link in release archive: {}",
                path.display()
            )));
        }
        if kind.is_dir() {
            append_tar_directory(archive, root, &path, prefix, target)?;
        } else if kind.is_file() {
            let relative = path.strip_prefix(root).expect("entry below archive root");
            let mut header = tar::Header::new_gnu();
            header.set_metadata(&entry.metadata()?);
            if let Some(mode) = release_mode(relative, target) {
                header.set_mode(mode);
            }
            archive.append_data(&mut header, prefix.join(relative), File::open(&path)?)?;
        }
    }
    Ok(())
}
