use std::{fs, io, path::Path};

#[cfg(windows)]
mod windows;

#[cfg(windows)]
use windows::remove_installation_root;

use sempre_state::{Document, Layout, Mode, Runtime, Store};

use crate::{ManagerError, service_deploy::unregister_command};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplicationUninstall {
    pub purged: bool,
    pub installation_removal_scheduled: bool,
}

pub async fn uninstall_application(
    layout: &Layout,
    purge: bool,
) -> Result<ApplicationUninstall, ManagerError> {
    if layout.mode != Mode::System {
        return Err(ManagerError::InvalidOperation(
            "application uninstall requires a system layout".into(),
        ));
    }
    sempre_service::require_installation_privileges()?;
    let store = Store::new(layout.clone());
    let operation = store.acquire_operation()?;
    sempre_service::uninstall().await?;
    crate::dns_capture::cleanup(&layout.resources).await?;

    let mut problems = Vec::new();
    let transparent = sempre_transparent::Controller::new(layout);
    if let Err(error) = transparent.cleanup().await {
        problems.push(format!("clean transparent networking: {error}"));
    }
    if let Some(capture) = transparent.capture()
        && let Err(error) = capture.unregister().await
    {
        problems.push(format!("remove gateway capture: {error}"));
    }
    if let Err(error) = unregister_command(layout) {
        problems.push(format!("remove command registration: {error}"));
    }
    if !purge && let Err(error) = retain_configuration(&store) {
        problems.push(format!("reset retained state: {error}"));
        drop(operation);
        return Err(incomplete(&problems));
    }
    drop(operation);

    let scheduled = match remove_application_files(layout, purge) {
        Ok(scheduled) => scheduled,
        Err(error) => {
            problems.push(error.to_string());
            false
        }
    };
    if problems.is_empty() {
        Ok(ApplicationUninstall {
            purged: purge,
            installation_removal_scheduled: scheduled,
        })
    } else {
        Err(incomplete(&problems))
    }
}

fn retain_configuration(store: &Store) -> Result<(), ManagerError> {
    store.initialize()?;
    store.update(|document| {
        reset_runtime_state(document);
        Ok(())
    })?;
    Ok(())
}

fn reset_runtime_state(document: &mut Document) {
    document.selected = None;
    document.active = None;
    document.pending = false;
    document.last_error = None;
    document.cores.clear();
    document.runtime = Runtime::default();
}

fn remove_application_files(layout: &Layout, purge: bool) -> Result<bool, ManagerError> {
    let mut problems = Vec::new();
    for path in [&layout.cores, &layout.ui, &layout.logs, &layout.runtime] {
        if let Err(error) = remove_tree(path) {
            problems.push(format!("remove {}: {error}", path.display()));
        }
    }
    if purge && let Err(error) = remove_tree(&layout.home) {
        problems.push(format!("remove {}: {error}", layout.home.display()));
    }
    let scheduled = if layout.root == layout.home {
        // The programs share a directory with the configuration a plain uninstall retains.
        for path in [
            layout.service_executable.clone(),
            crate::packaged_core_path(layout),
        ] {
            if let Err(error) = remove_file(&path) {
                problems.push(format!("remove {}: {error}", path.display()));
            }
        }
        false
    } else {
        match remove_installation_root(&layout.root) {
            Ok(scheduled) => scheduled,
            Err(error) => {
                problems.push(error.to_string());
                false
            }
        }
    };
    if problems.is_empty() {
        Ok(scheduled)
    } else {
        Err(incomplete(&problems))
    }
}

fn remove_file(path: &Path) -> Result<(), io::Error> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn remove_tree(path: &Path) -> Result<(), io::Error> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(not(windows))]
fn remove_installation_root(path: &Path) -> Result<bool, ManagerError> {
    remove_tree(path).map_err(|error| {
        ManagerError::io(
            format!("remove installation directory {}", path.display()),
            error,
        )
    })?;
    Ok(false)
}

fn incomplete(problems: &[String]) -> ManagerError {
    ManagerError::UninstallIncomplete(problems.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed() -> (tempfile::TempDir, Layout) {
        let directory = tempfile::tempdir().expect("directory");
        let layout = Layout::openwrt_at(directory.path());
        layout.ensure().expect("layout");
        for path in [
            &layout.service_executable,
            &crate::packaged_core_path(&layout),
            &layout.state,
        ] {
            fs::write(path, b"data").expect("file");
        }
        (directory, layout)
    }

    #[test]
    fn plain_openwrt_uninstall_removes_programs_and_keeps_configuration() {
        let (_directory, layout) = installed();
        remove_application_files(&layout, false).expect("remove");
        assert!(!layout.service_executable.exists());
        assert!(!crate::packaged_core_path(&layout).exists());
        assert!(!layout.cores.exists());
        assert!(layout.state.exists());
    }

    #[test]
    fn purged_openwrt_uninstall_removes_the_whole_directory() {
        let (_directory, layout) = installed();
        remove_application_files(&layout, true).expect("remove");
        assert!(!layout.home.exists());
    }
}
