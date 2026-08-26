use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

use sempre_state::{Document, Layout, PORTABLE_MARKER, Runtime, write_atomic};
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

pub use deploy::{DeployComponent, stage_deploy};
pub use install::stage_install;
pub use restore::{
    BundleKind, RestoreTransaction, stage_restore, validate_release, validate_snapshot,
};

const METADATA_NAME: &str = ".sempre-bundle.json";

#[derive(Debug)]
pub struct Export {
    pub archive: PathBuf,
    pub download_name: String,
}

#[derive(Debug, Error)]
pub enum BundleError {
    #[error("prepare snapshot layout: {0}")]
    Layout(#[source] sempre_state::LayoutError),
    #[error("{operation} {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("encode snapshot {name}: {source}")]
    Encode {
        name: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("decode snapshot {name}: {source}")]
    Decode {
        name: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid web configuration: {0}")]
    InvalidWebConfig(&'static str),
    #[error("invalid snapshot metadata: {0}")]
    InvalidMetadata(String),
    #[error("snapshot file is missing: {0}")]
    Missing(PathBuf),
    #[error("refuse symbolic link in snapshot: {0}")]
    SymbolicLink(PathBuf),
    #[error("archive snapshot: {0}")]
    Archive(#[from] zip::result::ZipError),
}

#[derive(Serialize)]
struct Metadata {
    schema: u32,
    kind: &'static str,
}

pub fn export(
    source: &Layout,
    document: &Document,
    executable: &Path,
) -> Result<Export, BundleError> {
    fs::create_dir_all(&source.runtime).map_err(|source_error| BundleError::Io {
        operation: "create export directory",
        path: source.runtime.clone(),
        source: source_error,
    })?;
    let staging = tempfile::Builder::new()
        .prefix("bundle-export-")
        .tempdir_in(&source.runtime)
        .map_err(|source_error| BundleError::Io {
            operation: "create snapshot staging directory",
            path: source.runtime.clone(),
            source: source_error,
        })?;
    let package_name = format!(
        "sempre-bundle-{}-{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let package = staging.path().join(&package_name);
    let target = Layout::at(&package);
    target.ensure().map_err(BundleError::Layout)?;
    export_directory(source, &target, document, executable)?;

    let archive = source
        .runtime
        .join(format!("{package_name}-{}.zip", Uuid::new_v4()));
    if let Err(error) = zip_directory(&archive, &package, &package_name) {
        let _ = fs::remove_file(&archive);
        return Err(error);
    }
    Ok(Export {
        archive,
        download_name: format!("{package_name}.zip"),
    })
}

/// Mark an already prepared portable directory as an installable release.
pub fn mark_release_directory(root: &Path) -> Result<(), BundleError> {
    write_bundle_marker(root, "release")
}

fn write_bundle_marker(root: &Path, kind: &'static str) -> Result<(), BundleError> {
    write_json(
        root.join(METADATA_NAME).as_path(),
        &Metadata { schema: 1, kind },
    )?;
    write_atomic(&root.join(PORTABLE_MARKER), b"", 0o600).map_err(|source| BundleError::Io {
        operation: "write portable marker",
        path: root.join(PORTABLE_MARKER),
        source,
    })
}

fn export_directory(
    source: &Layout,
    target: &Layout,
    document: &Document,
    executable: &Path,
) -> Result<(), BundleError> {
    copy_file(executable, &target.service_executable, true)?;
    for (from, to) in [
        (&source.resources, &target.resources),
        (&source.tools, &target.tools),
        (&source.cores, &target.cores),
        (&source.configs, &target.configs),
        (&source.subscriptions, &target.subscriptions),
        (&source.gateway, &target.gateway),
        (&source.ui, &target.ui),
    ] {
        copy_tree(from, to)?;
    }
    copy_optional_file(&source.tunnels, &target.tunnels)?;
    write_document(&target.state, document)?;
    write_web_config(&source.web_config, &target.web_config)?;
    write_bundle_marker(&target.root, "snapshot")?;
    write_restore_script(&target.root)?;
    Ok(())
}

fn write_document(path: &Path, document: &Document) -> Result<(), BundleError> {
    let mut snapshot = document.clone();
    snapshot.runtime = Runtime::default();
    write_json(path, &snapshot)
}

fn write_web_config(source: &Path, target: &Path) -> Result<(), BundleError> {
    let data = fs::read(source).map_err(|source_error| BundleError::Io {
        operation: "read web configuration",
        path: source.to_path_buf(),
        source: source_error,
    })?;
    let mut value: serde_json::Value =
        serde_json::from_slice(&data).map_err(|source_error| BundleError::Decode {
            name: "web configuration",
            source: source_error,
        })?;
    let object = value
        .as_object_mut()
        .ok_or(BundleError::InvalidWebConfig("root must be an object"))?;
    object.remove("password");
    write_json(target, &value)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), BundleError> {
    let mut data = serde_json::to_vec_pretty(value).map_err(|source| BundleError::Encode {
        name: "bundle file",
        source,
    })?;
    data.push(b'\n');
    write_atomic(path, &data, 0o600).map_err(|source| BundleError::Io {
        operation: "write snapshot file",
        path: path.to_path_buf(),
        source,
    })
}

fn copy_optional_file(source: &Path, target: &Path) -> Result<(), BundleError> {
    match source.symlink_metadata() {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(BundleError::SymbolicLink(source.to_path_buf()))
        }
        Ok(metadata) if metadata.is_file() => copy_file(source, target, false),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source_error) => Err(BundleError::Io {
            operation: "inspect optional snapshot file",
            path: source.to_path_buf(),
            source: source_error,
        }),
    }
}

pub(crate) fn copy_tree(source: &Path, target: &Path) -> Result<(), BundleError> {
    let metadata = match source.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source_error) => {
            return Err(BundleError::Io {
                operation: "inspect snapshot directory",
                path: source.to_path_buf(),
                source: source_error,
            });
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(BundleError::SymbolicLink(source.to_path_buf()));
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(target).map_err(|source_error| BundleError::Io {
        operation: "create snapshot directory",
        path: target.to_path_buf(),
        source: source_error,
    })?;
    let entries = fs::read_dir(source).map_err(|source_error| BundleError::Io {
        operation: "read snapshot directory",
        path: source.to_path_buf(),
        source: source_error,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source_error| BundleError::Io {
            operation: "read snapshot entry",
            path: source.to_path_buf(),
            source: source_error,
        })?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let metadata = entry.file_type().map_err(|source_error| BundleError::Io {
            operation: "inspect snapshot entry",
            path: from.clone(),
            source: source_error,
        })?;
        if metadata.is_symlink() {
            return Err(BundleError::SymbolicLink(from));
        }
        if metadata.is_dir() {
            copy_tree(&from, &to)?;
        } else if metadata.is_file() {
            copy_file(&from, &to, false)?;
        }
    }
    Ok(())
}

pub(crate) fn copy_file(source: &Path, target: &Path, executable: bool) -> Result<(), BundleError> {
    #[cfg(not(unix))]
    let _ = executable;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source_error| BundleError::Io {
            operation: "create snapshot parent directory",
            path: parent.to_path_buf(),
            source: source_error,
        })?;
    }
    fs::copy(source, target).map_err(|source_error| BundleError::Io {
        operation: "copy snapshot file",
        path: source.to_path_buf(),
        source: source_error,
    })?;
    #[cfg(unix)]
    if executable {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(target, fs::Permissions::from_mode(0o755)).map_err(|source_error| {
            BundleError::Io {
                operation: "set snapshot executable permissions",
                path: target.to_path_buf(),
                source: source_error,
            }
        })?;
    }
    Ok(())
}

fn write_restore_script(root: &Path) -> Result<(), BundleError> {
    let (name, content): (&str, &str) = if cfg!(windows) {
        (
            "restore.cmd",
            "@echo off\r\n\"%~dp0sempre.exe\" bundle restore %*\r\npause\r\n",
        )
    } else {
        (
            "restore.sh",
            concat!(
                "#!/bin/sh\n",
                "SCRIPT_DIR=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd) || exit 1\n",
                "exec \"$SCRIPT_DIR/sempre\" bundle restore \"$@\"\n"
            ),
        )
    };
    let path = root.join(name);
    write_atomic(&path, content.as_bytes(), 0o755).map_err(|source| BundleError::Io {
        operation: "write snapshot restore script",
        path,
        source,
    })
}

fn zip_directory(destination: &Path, source: &Path, prefix: &str) -> Result<(), BundleError> {
    let file = File::create(destination).map_err(|source_error| BundleError::Io {
        operation: "create snapshot archive",
        path: destination.to_path_buf(),
        source: source_error,
    })?;
    let mut archive = ZipWriter::new(file);
    append_directory(&mut archive, source, source, prefix)?;
    archive.finish()?;
    Ok(())
}

fn append_directory(
    archive: &mut ZipWriter<File>,
    root: &Path,
    directory: &Path,
    prefix: &str,
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
            append_directory(archive, root, &path, prefix)?;
        } else if metadata.is_file() {
            let mode = archive_mode(&path)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use sempre_state::{RuntimeState, Store};

    #[test]
    fn exports_portable_snapshot_without_runtime_or_password() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let layout = Layout::at(temporary.path());
        let store = Store::new(layout.clone());
        let mut document = store.initialize().expect("initialize state");
        document.runtime.state = RuntimeState::Running;
        document.runtime.pid = Some(42);
        fs::write(
            &layout.web_config,
            r#"{"schema":1,"listen":"127.0.0.1:33211","password":{"key":"secret"}}"#,
        )
        .expect("write web configuration");
        fs::write(&layout.subscription_catalog, b"catalog").expect("write subscription catalog");
        let executable = temporary.path().join("current-sempre");
        fs::write(&executable, b"executable").expect("write executable");

        let result = export(&layout, &document, &executable).expect("export bundle");
        let archive = File::open(&result.archive).expect("open archive");
        let mut archive = zip::ZipArchive::new(archive).expect("read archive");
        let prefix = format!(
            "sempre-bundle-{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        assert!(
            archive
                .by_name(&format!("{prefix}/{METADATA_NAME}"))
                .is_ok()
        );
        let (executable_name, restore_name, restore_invocation) = if cfg!(windows) {
            ("sempre.exe", "restore.cmd", "%~dp0sempre.exe")
        } else {
            ("sempre", "restore.sh", "$SCRIPT_DIR/sempre")
        };
        assert!(
            archive
                .by_name(&format!("{prefix}/{executable_name}"))
                .is_ok()
        );
        let mut restore = archive
            .by_name(&format!("{prefix}/{restore_name}"))
            .expect("restore script");
        let mut restore_script = String::new();
        std::io::Read::read_to_string(&mut restore, &mut restore_script)
            .expect("read restore script");
        assert!(restore_script.contains(restore_invocation));
        drop(restore);
        let state: Document = read_json(&mut archive, &format!("{prefix}/.sempre/state.json"));
        assert_eq!(state.runtime, Runtime::default());
        let web: serde_json::Value = read_json(&mut archive, &format!("{prefix}/.sempre/web.json"));
        assert!(web.get("password").is_none());
        let mut catalog = archive
            .by_name(&format!("{prefix}/.sempre/subscriptions/catalog.json"))
            .expect("subscription catalog");
        let mut value = String::new();
        std::io::Read::read_to_string(&mut catalog, &mut value).expect("read catalog");
        assert_eq!(value, "catalog");
    }

    fn read_json<T: serde::de::DeserializeOwned>(
        archive: &mut zip::ZipArchive<File>,
        name: &str,
    ) -> T {
        let mut file = archive.by_name(name).expect("archive entry");
        serde_json::from_reader(&mut file).expect("decode archive entry")
    }
}
mod deploy;
mod install;
mod release;
mod restore;

pub use release::{ReleaseTarget, package_release};
