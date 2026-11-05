//! The dashboard a router's sing-box serves. sing-box fills an empty dashboard directory
//! by itself and skips the proxy rules, so Sempre keeps the directory non-empty and
//! downloads the dashboard through those rules instead.
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use sempre_artifact::{ArchiveFormat, ExtractOptions, extract};
use tokio::time::sleep;

use crate::{Manager, ManagerError, ValidationRunner, VersionRunner, supervisor::RuntimePlan};

const DASHBOARD_URL: &str =
    "https://github.com/MetaCubeX/metacubexd/archive/refs/heads/gh-pages.zip";
const USER_AGENT: &str = "sempre-client/dashboard";
const MAX_ARCHIVE_BYTES: usize = 64 << 20;
const FETCH_TIMEOUT: Duration = Duration::from_mins(5);
const RETRY_INTERVAL: Duration = Duration::from_mins(10);
const ENTRY: &str = "index.html";
const PLACEHOLDER: &str = "<!doctype html><meta charset=\"utf-8\"><title>Dashboard</title>\
<p>The dashboard has not been downloaded yet. Sempre retries through its proxy rules.</p>\n";

fn dashboard_directory(core_data: &Path) -> PathBuf {
    core_data.join("ui")
}

/// Run before the core starts, because it reads the directory once at startup.
pub(crate) fn prepare(core_data: &Path) -> Result<(), ManagerError> {
    let directory = dashboard_directory(core_data);
    let context = "prepare core dashboard";
    fs::create_dir_all(&directory).map_err(|error| ManagerError::io(context, error))?;
    let empty = fs::read_dir(&directory)
        .map_err(|error| ManagerError::io(context, error))?
        .next()
        .is_none();
    if empty {
        sempre_state::write_atomic(&directory.join(ENTRY), PLACEHOLDER.as_bytes(), 0o644)
            .map_err(|error| ManagerError::io(context, error))?;
    }
    Ok(())
}

fn awaiting_download(directory: &Path) -> bool {
    fs::read(directory.join(ENTRY)).is_ok_and(|content| content == PLACEHOLDER.as_bytes())
}

fn invalid(detail: impl Into<String>) -> ManagerError {
    ManagerError::InvalidOperation(format!("install core dashboard: {}", detail.into()))
}

/// A GitHub archive nests everything in one top-level directory.
fn dashboard_root(extracted: &Path) -> io::Result<Option<PathBuf>> {
    if extracted.join(ENTRY).is_file() {
        return Ok(Some(extracted.to_path_buf()));
    }
    for entry in fs::read_dir(extracted)? {
        let path = entry?.path();
        if path.join(ENTRY).is_file() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// The core reads files by path, so a replaced directory is served without a restart.
fn install(directory: &Path, archive: &[u8]) -> Result<(), ManagerError> {
    let context = "install core dashboard";
    let parent = directory
        .parent()
        .ok_or_else(|| invalid("the directory has no parent"))?;
    let work = tempfile::Builder::new()
        .prefix(".ui-")
        .tempdir_in(parent)
        .map_err(|error| ManagerError::io(context, error))?;
    let zip = work.path().join("dashboard.zip");
    fs::write(&zip, archive).map_err(|error| ManagerError::io(context, error))?;
    let extracted = work.path().join("extracted");
    extract(
        &zip,
        &extracted,
        &ExtractOptions {
            format: ArchiveFormat::Zip,
            single_file_name: None,
        },
    )
    .map_err(|error| invalid(error.to_string()))?;
    let root = dashboard_root(&extracted)
        .map_err(|error| ManagerError::io(context, error))?
        .ok_or_else(|| invalid(format!("the archive has no {ENTRY}")))?;
    // Extraction keeps its directory private, and the core serves this one as is.
    #[cfg(unix)]
    fs::set_permissions(&root, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .map_err(|error| ManagerError::io(context, error))?;
    let previous = work.path().join("previous");
    fs::rename(directory, &previous).map_err(|error| ManagerError::io(context, error))?;
    if let Err(error) = fs::rename(&root, directory) {
        let _ = fs::rename(&previous, directory);
        return Err(ManagerError::io(context, error));
    }
    Ok(())
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    /// Fills a router's dashboard while the core runs. Never returns.
    pub(crate) async fn maintain_core_dashboard(&self, plan: &RuntimePlan) {
        if self.store.layout().openwrt && plan.deployment.core == "sing-box" {
            let core_data = self.store.layout().core_data.join("sing-box");
            let directory = dashboard_directory(&core_data);
            while awaiting_download(&directory) {
                match self.download_core_dashboard(&directory).await {
                    Ok(()) => self.log_supervisor("core dashboard downloaded"),
                    Err(error) => {
                        self.log_supervisor(&format!("core dashboard download failed: {error}"));
                        sleep(RETRY_INTERVAL).await;
                    }
                }
            }
        }
        std::future::pending::<()>().await;
    }

    async fn download_core_dashboard(&self, directory: &Path) -> Result<(), ManagerError> {
        let archive = self
            .fetch_through_rules(DASHBOARD_URL, USER_AGENT, MAX_ARCHIVE_BYTES, FETCH_TIMEOUT)
            .await?;
        let directory = directory.to_path_buf();
        tokio::task::spawn_blocking(move || install(&directory, &archive))
            .await
            .map_err(|error| invalid(error.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn archive(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buffer = io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut buffer);
        for (name, content) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .expect("entry");
            writer.write_all(content.as_bytes()).expect("content");
        }
        writer.finish().expect("archive");
        buffer.into_inner()
    }

    #[test]
    fn an_empty_directory_gets_a_placeholder_and_waits_for_the_download() {
        let root = tempfile::tempdir().expect("root");
        prepare(root.path()).expect("prepare");
        assert!(awaiting_download(&dashboard_directory(root.path())));
    }

    #[test]
    fn a_downloaded_dashboard_is_left_alone() {
        let root = tempfile::tempdir().expect("root");
        let directory = dashboard_directory(root.path());
        fs::create_dir_all(&directory).expect("directory");
        fs::write(directory.join(ENTRY), "dashboard").expect("entry");
        prepare(root.path()).expect("prepare");
        assert!(!awaiting_download(&directory));
        assert_eq!(
            fs::read_to_string(directory.join(ENTRY)).expect("entry"),
            "dashboard"
        );
    }

    #[test]
    fn installing_unwraps_the_github_directory_and_ends_the_wait() {
        let root = tempfile::tempdir().expect("root");
        prepare(root.path()).expect("prepare");
        let directory = dashboard_directory(root.path());
        install(
            &directory,
            &archive(&[("site-gh-pages/index.html", "dashboard")]),
        )
        .expect("install");
        assert!(!awaiting_download(&directory));
        assert_eq!(
            fs::read_to_string(directory.join(ENTRY)).expect("entry"),
            "dashboard"
        );
    }

    #[test]
    fn an_archive_without_a_dashboard_keeps_waiting() {
        let root = tempfile::tempdir().expect("root");
        prepare(root.path()).expect("prepare");
        let directory = dashboard_directory(root.path());
        assert!(install(&directory, &archive(&[("site/readme.txt", "no")])).is_err());
        assert!(awaiting_download(&directory));
    }
}
