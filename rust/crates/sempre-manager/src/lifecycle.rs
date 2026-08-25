use std::{fs, path::PathBuf};

use sempre_core::CoreRef;
use sempre_state::{CoreState, Deployment, Document, Selection, SourceState};
use serde::Serialize;

use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CoreChange {
    pub changed: bool,
    pub needs_restart: bool,
    pub message: String,
    pub previous_detail: String,
    pub current_detail: String,
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn select_core(&self, value: &str) -> Result<CoreChange, ManagerError> {
        let _operation = self.store.acquire_operation()?;
        let reference = self.normalized_reference(value)?;
        let before = self.store.read()?;
        let version = resolve_installed_version(&before, &reference)?;
        let config_hash = before.configs.get(&reference.core).cloned();
        if let Some(hash) = &config_hash {
            let config = self.store.layout().config(&reference.core, hash);
            self.validate_config_path(&reference, &version, &config)
                .await
                .map_err(|source| ManagerError::CandidateRejected {
                    reference: reference.to_string(),
                    source: Box::new(source),
                })?;
        }

        let mut change = CoreChange::default();
        self.store.update_checked(|document| {
            let current_version = resolve_installed_version(document, &reference)?;
            if current_version != version
                || document.configs.get(&reference.core) != config_hash.as_ref()
            {
                return Err(ManagerError::CoreStateChanged {
                    operation: "selecting",
                    reference: reference.to_string(),
                });
            }
            let source = source_mut(document, &reference)?;
            let installation = source
                .installed
                .get_mut(&version)
                .ok_or_else(|| ManagerError::NotInstalled(reference.to_string()))?;
            if !reference.is_channel() && !installation.explicit {
                installation.explicit = true;
                change.changed = true;
            }
            let selection = Selection {
                core: reference.core.clone(),
                repository: reference.repository.clone(),
                reference: reference.reference.clone(),
            };
            if document.selected.as_ref() != Some(&selection) {
                document.selected = Some(selection);
                change.changed = true;
            }
            let Some(config_hash) = &config_hash else {
                change.current_detail = format!("{reference} (waiting for configuration)");
                return Ok(());
            };
            let deployment = Deployment {
                core: reference.core.clone(),
                repository: reference.repository.clone(),
                reference: reference.reference.clone(),
                version: version.clone(),
                config_hash: config_hash.clone(),
            };
            if document.active.as_ref() != Some(&deployment) {
                if let Some(active) = &document.active {
                    change.previous_detail = deployment_label(active);
                }
                change.current_detail = deployment_label(&deployment);
                change.changed = true;
                change.needs_restart = true;
                document.stage(deployment);
            }
            Ok(())
        })?;
        change.message = if change.changed {
            "selected core changed"
        } else {
            "core selection is already current"
        }
        .into();
        Ok(change)
    }
}

impl<R: VersionRunner> Manager<R> {
    pub fn remove_core(&self, value: &str) -> Result<CoreChange, ManagerError> {
        let _operation = self.store.acquire_operation()?;
        let reference = self.normalized_reference(value)?;
        let before = self.store.read()?;
        let version = resolve_installed_version(&before, &reference)?;
        reject_referenced_version(&before, &reference, &version)?;

        let version_directory = self.store.layout().core_version_dir(
            &reference.core,
            reference.repository.as_deref(),
            &version,
        );
        let removed = stage_removal(&version_directory, &version)?;
        let result = self.store.update_checked(|document| {
            let current = resolve_installed_version(document, &reference)?;
            if current != version
                || reject_referenced_version(document, &reference, &version).is_err()
            {
                return Err(ManagerError::CoreStateChanged {
                    operation: "removing",
                    reference: exact_reference(&reference, &version),
                });
            }
            remove_installation(document, &reference, &version)?;
            Ok(())
        });
        if let Err(error) = result {
            if let Some(removed) = removed {
                let _ = fs::rename(removed, &version_directory);
            }
            return Err(error);
        }
        if let Some(removed) = removed {
            fs::remove_dir_all(&removed)
                .map_err(|error| ManagerError::io("clean removed core files", error))?;
        }
        Ok(CoreChange {
            changed: true,
            message: format!("{} removed", exact_reference(&reference, &version)),
            ..CoreChange::default()
        })
    }

    pub(crate) fn normalized_reference(&self, value: &str) -> Result<CoreRef, ManagerError> {
        let mut reference = CoreRef::parse(value)?;
        let adapter = self.registry.get(&reference.core)?;
        if reference
            .repository
            .as_deref()
            .is_some_and(|repository| repository.eq_ignore_ascii_case(adapter.default_repository()))
        {
            reference.repository = None;
        }
        Ok(reference)
    }
}

pub(crate) fn resolve_installed_version(
    document: &Document,
    reference: &CoreRef,
) -> Result<String, ManagerError> {
    let source = source(document, reference)?;
    let version = if reference.is_channel() {
        source.channels.get(&reference.reference)
    } else {
        Some(&reference.reference)
    };
    version
        .filter(|version| source.installed.contains_key(*version))
        .cloned()
        .ok_or_else(|| ManagerError::NotInstalled(reference.to_string()))
}

fn source<'a>(
    document: &'a Document,
    reference: &CoreRef,
) -> Result<&'a SourceState, ManagerError> {
    let core = document
        .cores
        .get(&reference.core)
        .ok_or_else(|| ManagerError::NotInstalled(reference.to_string()))?;
    match reference.repository.as_deref() {
        Some(repository) => core.custom.get(repository),
        None => Some(&core.default),
    }
    .ok_or_else(|| ManagerError::NotInstalled(reference.to_string()))
}

fn source_mut<'a>(
    document: &'a mut Document,
    reference: &CoreRef,
) -> Result<&'a mut SourceState, ManagerError> {
    let core = document
        .cores
        .get_mut(&reference.core)
        .ok_or_else(|| ManagerError::NotInstalled(reference.to_string()))?;
    match reference.repository.as_deref() {
        Some(repository) => core.custom.get_mut(repository),
        None => Some(&mut core.default),
    }
    .ok_or_else(|| ManagerError::NotInstalled(reference.to_string()))
}

fn reject_referenced_version(
    document: &Document,
    reference: &CoreRef,
    version: &str,
) -> Result<(), ManagerError> {
    let exact = exact_reference(reference, version);
    if selection_references(document, reference, version) {
        return Err(ManagerError::CoreInUse {
            reference: exact,
            usage: "selected",
        });
    }
    for (deployment, usage) in [
        (document.active.as_ref(), "active"),
        (document.previous.as_ref(), "retained for rollback"),
    ] {
        if deployment.is_some_and(|item| deployment_references(item, reference, version)) {
            return Err(ManagerError::CoreInUse {
                reference: exact,
                usage,
            });
        }
    }
    Ok(())
}

fn selection_references(document: &Document, reference: &CoreRef, version: &str) -> bool {
    let Some(selection) = &document.selected else {
        return false;
    };
    if selection.core != reference.core || selection.repository != reference.repository {
        return false;
    }
    let selected = CoreRef {
        core: selection.core.clone(),
        repository: selection.repository.clone(),
        reference: selection.reference.clone(),
    };
    resolve_installed_version(document, &selected).is_ok_and(|resolved| resolved == version)
}

fn deployment_references(deployment: &Deployment, reference: &CoreRef, version: &str) -> bool {
    deployment.core == reference.core
        && deployment.repository == reference.repository
        && deployment.version == version
}

fn remove_installation(
    document: &mut Document,
    reference: &CoreRef,
    version: &str,
) -> Result<(), ManagerError> {
    let core = document
        .cores
        .get_mut(&reference.core)
        .ok_or_else(|| ManagerError::NotInstalled(reference.to_string()))?;
    let source = match reference.repository.as_deref() {
        Some(repository) => core.custom.get_mut(repository),
        None => Some(&mut core.default),
    }
    .ok_or_else(|| ManagerError::NotInstalled(reference.to_string()))?;
    source.channels.retain(|_, target| target != version);
    source.installed.remove(version);
    if let Some(repository) = reference.repository.as_deref()
        && source.channels.is_empty()
        && source.installed.is_empty()
    {
        core.custom.remove(repository);
    }
    if core_is_empty(core) {
        document.cores.remove(&reference.core);
    }
    Ok(())
}

fn core_is_empty(core: &CoreState) -> bool {
    core.default.channels.is_empty()
        && core.default.installed.is_empty()
        && core
            .custom
            .values()
            .all(|source| source.channels.is_empty() && source.installed.is_empty())
}

fn stage_removal(
    directory: &std::path::Path,
    version: &str,
) -> Result<Option<PathBuf>, ManagerError> {
    if !directory.exists() {
        return Ok(None);
    }
    let parent = directory
        .parent()
        .ok_or_else(|| ManagerError::io("locate core version parent", invalid_path()))?;
    let temporary = tempfile::Builder::new()
        .prefix(&format!(".remove-{version}-"))
        .tempdir_in(parent)
        .map_err(|error| ManagerError::io("prepare core removal", error))?;
    let removed = temporary.path().to_owned();
    temporary
        .close()
        .map_err(|error| ManagerError::io("prepare core removal", error))?;
    fs::rename(directory, &removed)
        .map_err(|error| ManagerError::io("prepare core removal", error))?;
    Ok(Some(removed))
}

fn exact_reference(reference: &CoreRef, version: &str) -> String {
    let mut exact = reference.clone();
    exact.reference = version.into();
    exact.to_string()
}

fn deployment_label(deployment: &Deployment) -> String {
    let reference = CoreRef {
        core: deployment.core.clone(),
        repository: deployment.repository.clone(),
        reference: deployment.reference.clone(),
    };
    format!("{reference} -> {}", deployment.version)
}

fn invalid_path() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "path has no usable parent",
    )
}

#[cfg(test)]
mod tests;
