use std::{fs, io::Write, path::Path};

use sempre_core::CoreRef;
use sempre_state::{Document, Selection};
use serde::Serialize;

use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

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

fn selection_reference(selection: &Selection) -> CoreRef {
    CoreRef {
        core: selection.core.clone(),
        repository: selection.repository.clone(),
        reference: selection.reference.clone(),
    }
}

#[cfg(test)]
mod tests;
