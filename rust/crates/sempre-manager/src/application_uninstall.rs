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

    let mut problems = Vec::new();
    if let Err(error) = sempre_transparent::Controller::new(layout).cleanup().await {
        problems.push(format!("clean transparent networking: {error}"));
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
    document.previous = None;
    document.previous_config_build = None;
    document.previous_profile_id = None;
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
    let scheduled = match remove_installation_root(&layout.root) {
        Ok(scheduled) => scheduled,
        Err(error) => {
            problems.push(error.to_string());
            false
        }
    };
    if problems.is_empty() {
        Ok(scheduled)
    } else {
        Err(incomplete(&problems))
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
    use std::collections::BTreeMap;

    use sempre_state::{ConfigBuild, CoreState, RuntimeState};

    use super::*;

    #[tokio::test]
    async fn application_uninstall_rejects_non_system_layouts() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let layout = Layout::at(temporary.path());
        assert!(matches!(
            uninstall_application(&layout, false).await,
            Err(ManagerError::InvalidOperation(_))
        ));
    }

    #[test]
    fn retained_state_forgets_absent_cores_but_keeps_configuration() {
        let mut document = Document::default();
        document
            .cores
            .insert("sing-box".into(), CoreState::default());
        document.pending = true;
        document.last_error = Some("failed".into());
        document.runtime.state = RuntimeState::Running;
        document.runtime.pid = Some(42);
        document.configs.insert("sing-box".into(), "a".repeat(64));
        document.config_builds.insert(
            "sing-box".into(),
            ConfigBuild {
                profile_id: "default".into(),
                profile_revision: 1,
                target_key: "sing-box".into(),
                runtime_key: None,
            },
        );
        document.active_profile_id = Some("default".into());
        document.subscription.url = Some("https://example.com/sub".into());
        let expected_configs = document.configs.clone();
        let expected_builds = document.config_builds.clone();

        reset_runtime_state(&mut document);

        assert_eq!(document.cores, BTreeMap::new());
        assert_eq!(document.runtime, Runtime::default());
        assert!(!document.pending);
        assert!(document.last_error.is_none());
        assert_eq!(document.configs, expected_configs);
        assert_eq!(document.config_builds, expected_builds);
        assert_eq!(document.active_profile_id.as_deref(), Some("default"));
        assert_eq!(
            document.subscription.url.as_deref(),
            Some("https://example.com/sub")
        );
    }

    #[test]
    fn application_files_are_retained_or_purged_explicitly() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let retained = Layout::system_at(&temporary.path().join("retained"));
        seed_layout(&retained);
        assert!(!remove_application_files(&retained, false).expect("retain configuration"));
        assert!(retained.state.exists());
        assert!(retained.web_config.exists());
        assert!(retained.subscription_catalog.exists());
        assert!(!retained.cores.exists());
        assert!(!retained.ui.exists());
        assert!(!retained.logs.exists());
        assert!(!retained.runtime.exists());
        assert!(!retained.root.exists());

        let purged = Layout::system_at(&temporary.path().join("purged"));
        seed_layout(&purged);
        assert!(!remove_application_files(&purged, true).expect("purge data"));
        assert!(!purged.home.exists());
        assert!(!purged.logs.exists());
        assert!(!purged.runtime.exists());
        assert!(!purged.root.exists());
    }

    fn seed_layout(layout: &Layout) {
        layout.ensure().expect("layout directories");
        fs::create_dir_all(&layout.root).expect("installation root");
        fs::create_dir_all(&layout.ui).expect("UI directory");
        fs::write(&layout.state, b"state").expect("state");
        fs::write(&layout.web_config, b"web").expect("web configuration");
        fs::write(&layout.subscription_catalog, b"subscriptions").expect("subscriptions");
        fs::write(&layout.service_executable, b"binary").expect("binary");
        fs::write(layout.cores.join("core"), b"core").expect("core");
        fs::write(layout.ui.join("index.html"), b"ui").expect("UI");
        fs::write(&layout.manager_log, b"log").expect("log");
        fs::write(layout.runtime.join("runtime"), b"runtime").expect("runtime");
    }
}
