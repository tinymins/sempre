//! `OpenWrt` gets sing-box from a release package or the firmware, so Sempre adopts that
//! binary instead of downloading.
use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use sempre_state::{Document, Installation, Layout, Selection};

use crate::{Manager, ManagerError, VersionRunner};

const FIRMWARE_CORE: &str = "sing-box";
const FIRMWARE_CORE_PATHS: [&str; 2] = ["/usr/bin/sing-box", "/etc/sing-box/sing-box"];
const FIRMWARE_SOURCE: &str = "firmware";
const STABLE: &str = "stable";

impl<R: VersionRunner> Manager<R> {
    pub(crate) fn reject_firmware_managed_core(&self, core: &str) -> Result<(), ManagerError> {
        if self.store.layout().openwrt && core != FIRMWARE_CORE {
            return Err(ManagerError::FirmwareManagedCore(core.into()));
        }
        Ok(())
    }

    /// The package or firmware supplies this one and would bring it back on the next start.
    pub(crate) fn reject_removing_system_core(
        &self,
        document: &Document,
        core: &str,
        version: &str,
    ) -> Result<(), ManagerError> {
        let installed = document
            .cores
            .get(core)
            .and_then(|state| state.default.installed.get(version));
        if self.store.layout().openwrt
            && installed.is_some_and(|item| item.source == FIRMWARE_SOURCE)
        {
            return Err(ManagerError::InvalidOperation(format!(
                "{core} {version} comes with the installed package or firmware"
            )));
        }
        Ok(())
    }

    /// Registers the firmware binary as the installed core; a no-op on other hosts.
    pub async fn adopt_firmware_core(&self) -> Result<(), ManagerError> {
        if !self.store.layout().openwrt {
            return Ok(());
        }
        let binary = system_core_binary(self.store.layout())
            .ok_or_else(|| ManagerError::FirmwareCoreMissing(FIRMWARE_CORE.into()))?;
        self.adopt_core_binary(&binary).await
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

/// Where a release package installs its core, beside the service executable.
pub fn packaged_core_path(layout: &Layout) -> PathBuf {
    layout.service_executable.with_file_name(FIRMWARE_CORE)
}

/// Installing a package that carries a core is an explicit choice, so it outranks firmware.
fn system_core_binary(layout: &Layout) -> Option<PathBuf> {
    std::iter::once(packaged_core_path(layout))
        .chain(FIRMWARE_CORE_PATHS.iter().map(PathBuf::from))
        .find(|path| path.is_file())
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
    for deployment in [
        &mut document.active,
        &mut document.last_good,
        &mut document.rejected_deployment,
    ]
    .into_iter()
    .flatten()
    {
        if deployment.core == FIRMWARE_CORE && stale.contains(&deployment.version) {
            version.clone_into(&mut deployment.version);
        }
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
