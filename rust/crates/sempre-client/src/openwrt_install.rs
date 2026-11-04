//! `OpenWrt` releases are one executable and a UI archive, with the proxy core supplied
//! by the firmware, so installation has no release bundle to validate or deploy.
use std::{
    fs,
    path::{Path, PathBuf},
};

use sempre_manager::Manager;
use sempre_state::{Layout, Mode, Store};

use crate::{ClientError, bootstrap};

const UI_ARCHIVE: &str = "sempre-ui.zip";

pub(crate) async fn run(options: bootstrap::Options<'_>) -> Result<(), ClientError> {
    sempre_service::require_installation_privileges()?;
    let source =
        std::env::current_exe().map_err(|error| io("locate executable", "sempre", error))?;
    let layout = Layout::for_mode(Mode::System)?;
    layout.ensure()?;
    install_executable(&source, &layout)?;
    let manager = Manager::new(Store::new(layout.clone()))?;
    install_adjacent_ui(&source, &layout)?;
    bootstrap::prepare(&manager, options).await?;
    sempre_service::install(&layout.service_executable, &layout.home).await?;
    sempre_service::start().await?;
    println!("Sempre installed, enabled, and started.");
    Ok(())
}

fn install_executable(source: &Path, layout: &Layout) -> Result<(), ClientError> {
    let target = &layout.service_executable;
    if source != target.as_path() {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io("create executable directory", parent, error))?;
        }
        let data = fs::read(source).map_err(|error| io("read executable", source, error))?;
        sempre_state::write_atomic(target, &data, 0o755)
            .map_err(|error| io("install executable", target, error))?;
    }
    let link = &layout.command_executable;
    if link != target {
        match fs::remove_file(link) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io("replace command link", link, error)),
        }
        symlink(target, link).map_err(|error| io("link command", link, error))?;
    }
    Ok(())
}

/// A failed or missing UI leaves the service usable through the CLI.
fn install_adjacent_ui(source: &Path, layout: &Layout) -> Result<(), ClientError> {
    let Some(archive) = source.parent().map(|parent| parent.join(UI_ARCHIVE)) else {
        return Ok(());
    };
    if archive.is_file() {
        let name = archive.display().to_string();
        sempre_ui::Store::new(&layout.ui).install_file(&archive, "local", &name, "")?;
    } else {
        println!("No {UI_ARCHIVE} next to the executable; the Web UI is not installed.");
    }
    Ok(())
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn symlink(_: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
}

fn io(operation: &'static str, path: impl Into<PathBuf>, source: std::io::Error) -> ClientError {
    ClientError::Io {
        operation,
        path: path.into(),
        source,
    }
}
