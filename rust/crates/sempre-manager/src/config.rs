use std::{fs, io::Write, path::Path};

use sempre_core::CoreRef;
use sempre_state::{ConfigBuild, Deployment, Document, Selection};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{CoreChange, Manager, ManagerError, ValidationRunner, VersionRunner};

pub const MAX_CONFIG_SIZE: usize = 32 << 20;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CurrentConfig {
    pub hash: String,
    pub content: String,
}

impl<R: VersionRunner> Manager<R> {
    pub fn current_config(&self) -> Result<CurrentConfig, ManagerError> {
        let document = self.store.read()?;
        let selection = document
            .selected
            .as_ref()
            .ok_or(ManagerError::NoSelectedCore)?;
        let hash = document
            .configs
            .get(&selection.core)
            .ok_or(ManagerError::NoConfiguration)?;
        let content = fs::read_to_string(self.store.layout().config(&selection.core, hash))
            .map_err(|error| ManagerError::io("read active configuration", error))?;
        Ok(CurrentConfig {
            hash: hash.clone(),
            content,
        })
    }
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn validate_config_content(&self, content: &[u8]) -> Result<(), ManagerError> {
        if content.len() > MAX_CONFIG_SIZE {
            return Err(ManagerError::ConfigurationTooLarge {
                limit: MAX_CONFIG_SIZE,
            });
        }
        let document = self.store.read()?;
        let (reference, version) = configuration_target(&document)?;
        let mut candidate = tempfile::Builder::new()
            .prefix("config-validate-")
            .suffix(".json")
            .tempfile_in(&self.store.layout().runtime)
            .map_err(|error| ManagerError::io("create configuration candidate", error))?;
        candidate
            .write_all(content)
            .map_err(|error| ManagerError::io("write configuration candidate", error))?;
        candidate
            .flush()
            .map_err(|error| ManagerError::io("flush configuration candidate", error))?;
        self.validate_config_path(&reference, &version, candidate.path())
            .await
    }

    pub(crate) async fn validate_config_path(
        &self,
        reference: &CoreRef,
        version: &str,
        config: &Path,
    ) -> Result<(), ManagerError> {
        let adapter = self.registry.get(&reference.core)?;
        let binary = self.store.layout().core_binary(
            &reference.core,
            reference.repository.as_deref(),
            version,
        );
        let data = tempfile::Builder::new()
            .prefix("validate-")
            .tempdir_in(&self.store.layout().runtime)
            .map_err(|error| ManagerError::io("create validation directory", error))?;
        self.runner
            .validate(adapter.as_ref(), &binary, config, data.path())
            .await
    }

    pub async fn activate_config_content(
        &self,
        content: &[u8],
        build: ConfigBuild,
    ) -> Result<CoreChange, ManagerError> {
        self.activate_config_content_updating(content, build, |_, _| {})
            .await
    }

    pub(crate) async fn activate_config_content_updating(
        &self,
        content: &[u8],
        build: ConfigBuild,
        update: impl FnOnce(&mut Document, bool),
    ) -> Result<CoreChange, ManagerError> {
        if content.len() > MAX_CONFIG_SIZE {
            return Err(ManagerError::ConfigurationTooLarge {
                limit: MAX_CONFIG_SIZE,
            });
        }
        let _config = self.store.acquire_config()?;
        let before = self.store.read()?;
        let (reference, version) = configuration_target(&before)?;
        let mut candidate = tempfile::Builder::new()
            .prefix("config-candidate-")
            .suffix(".json")
            .tempfile_in(&self.store.layout().runtime)
            .map_err(|error| ManagerError::io("create configuration candidate", error))?;
        candidate
            .write_all(content)
            .map_err(|error| ManagerError::io("write configuration candidate", error))?;
        candidate
            .flush()
            .map_err(|error| ManagerError::io("flush configuration candidate", error))?;
        self.validate_config_path(&reference, &version, candidate.path())
            .await?;

        let hash = format!("{:x}", Sha256::digest(content));
        let path = self.store.layout().config(&reference.core, &hash);
        let created = if path.exists() {
            let existing = fs::read(&path)
                .map_err(|error| ManagerError::io("read stored configuration", error))?;
            if existing != content {
                sempre_state::write_atomic(&path, content, 0o600)
                    .map_err(|error| ManagerError::io("repair stored configuration", error))?;
            }
            false
        } else {
            sempre_state::write_atomic(&path, content, 0o600)
                .map_err(|error| ManagerError::io("store configuration", error))?;
            true
        };

        let mut change = CoreChange::default();
        let update = self.store.update_checked(|document| {
            let (current, current_version) = configuration_target(document)?;
            if current != reference || current_version != version {
                return Err(ManagerError::CoreStateChanged {
                    operation: "activating configuration for",
                    reference: reference.to_string(),
                });
            }
            let old_hash = document
                .configs
                .get(&reference.core)
                .cloned()
                .unwrap_or_default();
            let config_changed = old_hash != hash;
            let runtime_changed = document
                .config_builds
                .get(&reference.core)
                .and_then(|current| current.runtime_key.as_ref())
                != build.runtime_key.as_ref();
            document
                .configs
                .insert(reference.core.clone(), hash.clone());
            document
                .config_builds
                .insert(reference.core.clone(), build.clone());
            let deployment = Deployment {
                core: reference.core.clone(),
                repository: reference.repository.clone(),
                reference: reference.reference.clone(),
                version: version.clone(),
                config_hash: hash.clone(),
            };
            let deployment_changed =
                document.active.as_ref() != Some(&deployment) || runtime_changed;
            if deployment_changed {
                document.stage(deployment);
                change.needs_restart = true;
            }
            change.changed = config_changed || deployment_changed;
            update(document, change.changed);
            change.previous_detail = short_hash(&old_hash);
            change.current_detail = short_hash(&hash);
            Ok(())
        });
        if let Err(error) = update {
            if created {
                let _ = fs::remove_file(path);
            }
            return Err(error);
        }
        change.message = if change.changed {
            "configuration updated and validated"
        } else {
            "configuration is already current"
        }
        .into();
        Ok(change)
    }
}

fn configuration_target(document: &Document) -> Result<(CoreRef, String), ManagerError> {
    let selection = document
        .selected
        .as_ref()
        .ok_or(ManagerError::NoSelectedCore)?;
    let reference = selection_reference(selection);
    let source = document
        .cores
        .get(&reference.core)
        .and_then(|core| match reference.repository.as_deref() {
            Some(repository) => core.custom.get(repository),
            None => Some(&core.default),
        })
        .ok_or_else(|| ManagerError::NotInstalled(reference.to_string()))?;
    let version = if reference.is_channel() {
        source.channels.get(&reference.reference)
    } else {
        Some(&reference.reference)
    };
    let version = version
        .filter(|version| source.installed.contains_key(*version))
        .cloned()
        .ok_or_else(|| ManagerError::NotInstalled(reference.to_string()))?;
    Ok((reference, version))
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(12).collect()
}

fn selection_reference(selection: &Selection) -> CoreRef {
    CoreRef {
        core: selection.core.clone(),
        repository: selection.repository.clone(),
        reference: selection.reference.clone(),
    }
}

#[cfg(test)]
mod tests;
