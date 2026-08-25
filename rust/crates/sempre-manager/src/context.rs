use sempre_core::{Capabilities, CompilerTarget, CoreRef};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{Manager, ManagerError, VersionRunner};

#[derive(Clone, Debug, Serialize)]
pub struct ConfigurationContext {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<ConfigurationTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running: Option<RunningCore>,
    pub platform: String,
    pub capabilities: Capabilities,
}

#[derive(Clone, Debug, Serialize)]
pub struct ConfigurationTarget {
    pub core: String,
    pub version: String,
    pub compiler_target: CompilerTarget,
    pub key: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunningCore {
    pub core: String,
    pub version: String,
}

impl<R: VersionRunner> Manager<R> {
    pub fn configuration_context(&self) -> Result<ConfigurationContext, ManagerError> {
        let document = self.store.read()?;
        let running = document.active.as_ref().map(|active| RunningCore {
            core: active.core.clone(),
            version: active.version.clone(),
        });
        let Some(selection) = &document.selected else {
            return Ok(ConfigurationContext {
                key: "common".into(),
                target: None,
                running,
                platform: std::env::consts::OS.into(),
                capabilities: self.registry.stable_capabilities(&self.target),
            });
        };
        let reference = CoreRef {
            core: selection.core.clone(),
            repository: selection.repository.clone(),
            reference: selection.reference.clone(),
        };
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
        }
        .filter(|version| source.installed.contains_key(*version))
        .cloned()
        .ok_or_else(|| ManagerError::NotInstalled(reference.to_string()))?;
        let adapter = self.registry.get(&reference.core)?;
        let compiler_target = adapter.compiler_target(Some(&version), &self.target)?;
        let key_data = format!(
            "{}|{}|{}|{}",
            reference.core, version, compiler_target.format, compiler_target.platform
        );
        let key = format!("{:x}", Sha256::digest(key_data.as_bytes()));
        Ok(ConfigurationContext {
            key: key.clone(),
            target: Some(ConfigurationTarget {
                core: reference.core,
                version: version.clone(),
                compiler_target,
                key,
            }),
            running,
            platform: std::env::consts::OS.into(),
            capabilities: adapter.capabilities(Some(&version), &self.target),
        })
    }
}

#[cfg(test)]
mod tests;
