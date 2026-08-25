use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::Utc;
use sempre_artifact::{ArchiveFormat, Artifact, ExtractOptions, extract, find};
use sempre_core::{Adapter, CoreRef, Package};
use sempre_state::{Document, Installation};
use serde::Serialize;

use crate::{Manager, ManagerError, VersionRunner};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InstallResult {
    pub core: String,
    pub repository: Option<String>,
    pub reference: String,
    pub version: String,
    pub binary: PathBuf,
    pub installed: bool,
    pub changed: bool,
}

impl<R: VersionRunner> Manager<R> {
    pub async fn install_core(&self, value: &str) -> Result<InstallResult, ManagerError> {
        let _operation = self.store.acquire_operation()?;
        let mut reference = CoreRef::parse(value)?;
        let adapter = self.registry.get(&reference.core)?;
        if reference
            .repository
            .as_deref()
            .is_some_and(|repository| repository.eq_ignore_ascii_case(adapter.default_repository()))
        {
            reference.repository = None;
        }
        let package = self
            .releases
            .resolve(
                adapter.as_ref(),
                reference.repository.as_deref().unwrap_or_default(),
                &reference.reference,
                &self.target,
            )
            .await?;
        self.reject_conflicting_source(&reference, &package)?;

        let temporary = tempfile::Builder::new()
            .prefix("core-install-")
            .tempdir_in(&self.store.layout().runtime)
            .map_err(|error| ManagerError::io("create core install directory", error))?;
        let archive = temporary.path().join(&package.name);
        self.downloader
            .verified(
                &Artifact {
                    name: package.name.clone(),
                    url: package.url.clone(),
                    digest: package.digest.clone(),
                    size: package.size,
                },
                &archive,
            )
            .await?;
        self.install_downloaded(&reference, adapter, &package, &archive)
            .await
    }

    async fn install_downloaded(
        &self,
        reference: &CoreRef,
        adapter: Arc<dyn Adapter>,
        package: &Package,
        archive: &Path,
    ) -> Result<InstallResult, ManagerError> {
        self.reject_conflicting_source(reference, package)?;
        let executable = adapter.executable_name(&self.target)?;
        let final_directory = self.store.layout().core_version_dir(
            adapter.id(),
            reference.repository.as_deref(),
            &package.version,
        );
        let final_binary = final_directory.join(&executable);
        let installed = if final_binary.is_file() {
            self.verify_version(adapter.as_ref(), &final_binary, &package.version)
                .await?;
            false
        } else {
            let extracted = tempfile::Builder::new()
                .prefix("core-extract-")
                .tempdir_in(&self.store.layout().runtime)
                .map_err(|error| ManagerError::io("create extraction directory", error))?;
            extract(
                archive,
                extracted.path(),
                &ExtractOptions {
                    format: ArchiveFormat::try_from(package.format.as_str())?,
                    single_file_name: Some(executable.clone()),
                },
            )?;
            let source_binary = find(extracted.path(), &executable)?;
            let source_directory = source_binary
                .parent()
                .ok_or_else(|| ManagerError::io("locate extracted core", invalid_path()))?;
            let parent = final_directory
                .parent()
                .ok_or_else(|| ManagerError::io("locate core version parent", invalid_path()))?;
            fs::create_dir_all(parent)
                .map_err(|error| ManagerError::io("create core source directory", error))?;
            secure_directory(parent)?;
            let staging = tempfile::Builder::new()
                .prefix(&format!(".{}-", package.version))
                .tempdir_in(parent)
                .map_err(|error| ManagerError::io("create core staging directory", error))?;
            copy_tree(source_directory, staging.path())?;
            let staging_binary = staging.path().join(&executable);
            make_executable(&staging_binary)?;
            self.verify_version(adapter.as_ref(), &staging_binary, &package.version)
                .await?;
            let activated = activate(staging, &final_directory, &final_binary)?;
            if !activated {
                self.verify_version(adapter.as_ref(), &final_binary, &package.version)
                    .await?;
            }
            activated
        };

        let mut state_change = StateChange::default();
        let update = self.store.update(|document| {
            state_change = record_installation(document, reference, package);
            Ok(())
        });
        if let Err(error) = update {
            if installed {
                let _ = fs::remove_dir_all(&final_directory);
            }
            return Err(error.into());
        }
        if let Some(version) = &state_change.cleanup_version {
            let directory = self.store.layout().core_version_dir(
                adapter.id(),
                reference.repository.as_deref(),
                version,
            );
            let _ = fs::remove_dir_all(directory);
        }
        Ok(InstallResult {
            core: reference.core.clone(),
            repository: reference.repository.clone(),
            reference: reference.reference.clone(),
            version: package.version.clone(),
            binary: final_binary,
            installed,
            changed: installed || state_change.changed,
        })
    }

    async fn verify_version(
        &self,
        adapter: &dyn Adapter,
        binary: &Path,
        expected: &str,
    ) -> Result<(), ManagerError> {
        let actual = self.runner.version(adapter, binary).await?;
        if actual == expected {
            Ok(())
        } else {
            Err(ManagerError::VersionMismatch {
                core: adapter.id().into(),
                expected: expected.into(),
                actual,
            })
        }
    }

    fn reject_conflicting_source(
        &self,
        reference: &CoreRef,
        package: &Package,
    ) -> Result<(), ManagerError> {
        let document = self.store.read()?;
        let existing = installation(&document, reference, &package.version);
        if let Some(existing) =
            existing.filter(|item| !item.source.is_empty() && item.source != package.url)
        {
            let mut exact = reference.clone();
            exact.reference.clone_from(&package.version);
            return Err(ManagerError::ConflictingSource {
                reference: exact.to_string(),
                existing: existing.source.clone(),
                candidate: package.url.clone(),
            });
        }
        Ok(())
    }
}

#[derive(Default)]
struct StateChange {
    changed: bool,
    cleanup_version: Option<String>,
}

fn record_installation(
    document: &mut Document,
    reference: &CoreRef,
    package: &Package,
) -> StateChange {
    let mut changed = false;
    let previous_version = {
        let source = document
            .core_mut(&reference.core)
            .source_mut(reference.repository.as_deref());
        let previous_version = reference
            .is_channel()
            .then(|| source.channels.get(&reference.reference).cloned())
            .flatten();
        let installation = source
            .installed
            .entry(package.version.clone())
            .or_insert_with(|| {
                changed = true;
                Installation {
                    explicit: false,
                    digest: package.digest.clone(),
                    source: package.url.clone(),
                    installed_at: Utc::now(),
                }
            });
        if installation.digest != package.digest {
            installation.digest.clone_from(&package.digest);
            changed = true;
        }
        if installation.source != package.url {
            installation.source.clone_from(&package.url);
            changed = true;
        }
        if reference.is_channel() {
            if source.channels.get(&reference.reference) != Some(&package.version) {
                source
                    .channels
                    .insert(reference.reference.clone(), package.version.clone());
                changed = true;
            }
        } else if !installation.explicit {
            installation.explicit = true;
            changed = true;
        }
        previous_version
    };
    let cleanup_version = previous_version.filter(|version| {
        version != &package.version && collect_weak_version(document, reference, version)
    });
    StateChange {
        changed: changed || cleanup_version.is_some(),
        cleanup_version,
    }
}

fn collect_weak_version(document: &mut Document, reference: &CoreRef, version: &str) -> bool {
    if version_is_referenced(document, reference, version) {
        return false;
    }
    let Some(core) = document.cores.get_mut(&reference.core) else {
        return false;
    };
    let source = match reference.repository.as_deref() {
        Some(repository) => core.custom.get_mut(repository),
        None => Some(&mut core.default),
    };
    let Some(source) = source else {
        return false;
    };
    let removable = source.installed.get(version).is_some_and(|installation| {
        !installation.explicit && source.channels.values().all(|value| value != version)
    });
    if !removable {
        return false;
    }
    source.installed.remove(version);
    if let Some(repository) = reference.repository.as_deref()
        && source.channels.is_empty()
        && source.installed.is_empty()
    {
        core.custom.remove(repository);
    }
    true
}

fn version_is_referenced(document: &Document, reference: &CoreRef, version: &str) -> bool {
    let selection_references = document.selected.as_ref().is_some_and(|selection| {
        if selection.core != reference.core || selection.repository != reference.repository {
            return false;
        }
        if selection.reference == version {
            return true;
        }
        let Some(core) = document.cores.get(&selection.core) else {
            return false;
        };
        let source = match selection.repository.as_deref() {
            Some(repository) => core.custom.get(repository),
            None => Some(&core.default),
        };
        source
            .and_then(|source| source.channels.get(&selection.reference))
            .is_some_and(|resolved| resolved == version)
    });
    selection_references
        || [&document.active, &document.previous]
            .into_iter()
            .flatten()
            .any(|deployment| {
                deployment.core == reference.core
                    && deployment.repository == reference.repository
                    && deployment.version == version
            })
}

fn installation<'a>(
    document: &'a Document,
    reference: &CoreRef,
    version: &str,
) -> Option<&'a Installation> {
    let core = document.cores.get(&reference.core)?;
    let source = match reference.repository.as_deref() {
        Some(repository) => core.custom.get(repository)?,
        None => &core.default,
    };
    source.installed.get(version)
}

fn activate(
    staging: tempfile::TempDir,
    final_directory: &Path,
    final_binary: &Path,
) -> Result<bool, ManagerError> {
    match fs::rename(staging.path(), final_directory) {
        Ok(()) => {
            let _ = staging.keep();
            Ok(true)
        }
        Err(_) if final_binary.is_file() => Ok(false),
        Err(error) => Err(ManagerError::io("activate core version", error)),
    }
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), ManagerError> {
    let mut pending = vec![source.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let relative = directory
            .strip_prefix(source)
            .map_err(|_| ManagerError::io("resolve extracted path", invalid_path()))?;
        let target_directory = destination.join(relative);
        fs::create_dir_all(&target_directory)
            .map_err(|error| ManagerError::io("create core staging directory", error))?;
        secure_directory(&target_directory)?;
        for entry in fs::read_dir(&directory)
            .map_err(|error| ManagerError::io("scan extracted core", error))?
        {
            let entry = entry.map_err(|error| ManagerError::io("scan extracted entry", error))?;
            let file_type = entry
                .file_type()
                .map_err(|error| ManagerError::io("inspect extracted entry", error))?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                copy_file(&entry.path(), &target_directory.join(entry.file_name()))?;
            }
        }
    }
    Ok(())
}

fn copy_file(source: &Path, target: &Path) -> Result<(), ManagerError> {
    let mut input =
        File::open(source).map_err(|error| ManagerError::io("open extracted core file", error))?;
    #[cfg(unix)]
    let permissions = input
        .metadata()
        .map_err(|error| ManagerError::io("inspect extracted core file", error))?
        .permissions();
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(permissions.mode() & 0o777);
    }
    let mut output = options
        .open(target)
        .map_err(|error| ManagerError::io("create staged core file", error))?;
    io::copy(&mut input, &mut output).map_err(|error| ManagerError::io("copy core file", error))?;
    output
        .flush()
        .map_err(|error| ManagerError::io("flush core file", error))?;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), ManagerError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|error| ManagerError::io("make core executable", error))
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<(), ManagerError> {
    Ok(())
}

#[cfg(unix)]
fn secure_directory(path: &Path) -> Result<(), ManagerError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| ManagerError::io("secure core directory", error))
}

#[cfg(not(unix))]
fn secure_directory(_: &Path) -> Result<(), ManagerError> {
    Ok(())
}

fn invalid_path() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "path has no usable parent")
}

#[cfg(test)]
mod tests;
