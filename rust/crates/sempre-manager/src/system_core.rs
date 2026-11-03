//! `OpenWrt` firmware ships sing-box, so Sempre adopts that binary instead of downloading.
use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use sempre_state::{Document, Installation, Selection};

use crate::{Manager, ManagerError, VersionRunner};

const FIRMWARE_CORE: &str = "sing-box";
const FIRMWARE_CORE_PATHS: [&str; 2] = ["/usr/bin/sing-box", "/etc/sing-box/sing-box"];
const FIRMWARE_SOURCE: &str = "firmware";
const STABLE: &str = "stable";

impl<R: VersionRunner> Manager<R> {
    pub(crate) fn reject_firmware_managed_core(&self, core: &str) -> Result<(), ManagerError> {
        if self.store.layout().openwrt && core == FIRMWARE_CORE {
            return Err(ManagerError::FirmwareManagedCore(core.into()));
        }
        Ok(())
    }

    /// Registers the firmware binary as the installed core; a no-op on other hosts.
    pub async fn adopt_firmware_core(&self) -> Result<(), ManagerError> {
        if !self.store.layout().openwrt {
            return Ok(());
        }
        let binary = FIRMWARE_CORE_PATHS
            .iter()
            .map(Path::new)
            .find(|path| path.is_file())
            .ok_or_else(|| ManagerError::FirmwareCoreMissing(FIRMWARE_CORE.into()))?;
        self.adopt_core_binary(binary).await
    }

    pub(crate) async fn adopt_core_binary(&self, binary: &Path) -> Result<(), ManagerError> {
        let adapter = self.registry.get(FIRMWARE_CORE)?;
        let version = self.runner.version(adapter.as_ref(), binary).await?;
        let link = self
            .store
            .layout()
            .core_binary(FIRMWARE_CORE, None, &version);
        link_core(binary, &link)?;
        let mut stale = Vec::new();
        self.store.update(|document| {
            stale = record_firmware_core(document, &version);
            Ok(())
        })?;
        for version in stale {
            let directory = self
                .store
                .layout()
                .core_version_dir(FIRMWARE_CORE, None, &version);
            let _ = fs::remove_dir_all(directory);
        }
        Ok(())
    }
}

fn link_core(binary: &Path, link: &Path) -> Result<(), ManagerError> {
    let directory = link.parent().map_or_else(PathBuf::new, Path::to_path_buf);
    fs::create_dir_all(&directory)
        .map_err(|error| ManagerError::io("create firmware core directory", error))?;
    match fs::remove_file(link) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(ManagerError::io("replace firmware core link", error)),
    }
    symlink(binary, link).map_err(|error| ManagerError::io("link firmware core", error))
}

#[cfg(unix)]
fn symlink(binary: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(binary, link)
}

#[cfg(not(unix))]
fn symlink(binary: &Path, link: &Path) -> std::io::Result<()> {
    fs::copy(binary, link).map(|_| ())
}

/// Replaces earlier firmware versions, so a firmware upgrade keeps the deployment valid.
fn record_firmware_core(document: &mut Document, version: &str) -> Vec<String> {
    let source = &mut document.core_mut(FIRMWARE_CORE).default;
    let stale = source
        .installed
        .iter()
        .filter(|(installed, item)| *installed != version && item.source == FIRMWARE_SOURCE)
        .map(|(installed, _)| installed.clone())
        .collect::<Vec<_>>();
    for installed in &stale {
        source.installed.remove(installed);
    }
    source
        .installed
        .entry(version.to_owned())
        .or_insert_with(|| Installation {
            explicit: true,
            digest: String::new(),
            source: FIRMWARE_SOURCE.into(),
            installed_at: Utc::now(),
        });
    source.channels.insert(STABLE.into(), version.to_owned());
    if let Some(active) = document
        .active
        .as_mut()
        .filter(|active| active.core == FIRMWARE_CORE && stale.contains(&active.version))
    {
        version.clone_into(&mut active.version);
    }
    if let Some(selected) = document
        .selected
        .as_mut()
        .filter(|selected| selected.core == FIRMWARE_CORE && stale.contains(&selected.reference))
    {
        version.clone_into(&mut selected.reference);
    }
    if document.selected.is_none() {
        document.selected = Some(Selection {
            core: FIRMWARE_CORE.into(),
            repository: None,
            reference: STABLE.into(),
        });
    }
    stale
}

#[cfg(test)]
#[path = "system_core_tests.rs"]
mod tests;
