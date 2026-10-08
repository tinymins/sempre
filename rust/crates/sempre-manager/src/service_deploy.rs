use std::{fs, io, path::Path};

use sempre_bundle::BundleKind;
use sempre_service::State;
use sempre_state::{Document, Layout, Mode, Runtime};

use crate::{Manager, ManagerError, VersionRunner};

pub async fn uninstall_system_service(layout: &Layout) -> Result<(), ManagerError> {
    if layout.mode != Mode::System {
        return Err(ManagerError::InvalidOperation(
            "native service uninstall requires a system layout".into(),
        ));
    }
    sempre_service::require_installation_privileges()?;
    sempre_service::uninstall().await?;
    let transparent = sempre_transparent::Controller::new(layout).cleanup().await;
    let command = unregister_command(layout);
    transparent?;
    command
}

impl<R: VersionRunner> Manager<R> {
    pub async fn restore_bundle(
        &self,
        target: &Layout,
        allow_replace: bool,
        expected_version: &str,
    ) -> Result<(), ManagerError> {
        self.deploy_bundle(
            target,
            allow_replace,
            BundleKind::Snapshot,
            expected_version,
        )
        .await
    }

    pub async fn install_release(
        &self,
        target: &Layout,
        allow_replace: bool,
        expected_version: &str,
    ) -> Result<(), ManagerError> {
        self.deploy_bundle(target, allow_replace, BundleKind::Release, expected_version)
            .await
    }

    async fn deploy_bundle(
        &self,
        target: &Layout,
        allow_replace: bool,
        kind: BundleKind,
        expected_version: &str,
    ) -> Result<(), ManagerError> {
        let source = self.store.layout();
        if source.mode != Mode::Portable {
            return Err(ManagerError::InvalidOperation(format!(
                "{} deployment must run from an extracted portable bundle",
                kind.name()
            )));
        }
        match kind {
            BundleKind::Release => sempre_bundle::validate_release(&source.root)?,
            BundleKind::Snapshot => sempre_bundle::validate_snapshot(&source.root)?,
        }
        sempre_control::WebConfigStore::new(&source.web_config).read()?;
        sempre_service::require_installation_privileges()?;
        require_bundle_replacement_confirmation(kind, source, target, allow_replace)?;
        prepare_command_registration(target)?;
        let _operation = self.store.acquire_operation()?;
        let document = self.store.read()?;
        self.repair_and_validate_deployed_cores(source, &document, kind.name())
            .await?;
        let mut transaction = match kind {
            BundleKind::Release => sempre_bundle::stage_install(source, target)?,
            BundleKind::Snapshot => sempre_bundle::stage_restore(source, target)?,
        };
        let previous_service = sempre_service::status().await?;
        if !matches!(previous_service, State::NotInstalled | State::Stopped) {
            sempre_service::stop().await?;
        }
        if let Err(error) =
            crate::dns_capture::cleanup_for_deployment(&target.resources, &transaction).await
        {
            restore_service_state(previous_service).await;
            return Err(error);
        }
        if let Err(error) = transaction.activate() {
            restore_service_state(previous_service).await;
            return Err(error.into());
        }
        if let Err(error) = target.ensure() {
            transaction.rollback();
            restore_service_state(previous_service).await;
            return Err(error.into());
        }
        let mut command_created = false;
        let result = async {
            sempre_service::install(&target.service_executable, &target.home).await?;
            command_created = register_command(target)?;
            sempre_service::start().await?;
            crate::service_readiness::wait_for_health(
                target,
                expected_version,
                std::time::Duration::from_secs(30),
            )
            .await?;
            Ok::<(), ManagerError>(())
        }
        .await;
        if let Err(error) = result {
            let _ = sempre_service::stop().await;
            if command_created {
                let _ = unregister_command(target);
            }
            transaction.rollback();
            restore_registration_after_rollback(target, previous_service).await;
            return Err(error);
        }
        transaction.commit()?;
        Ok(())
    }
}

fn require_bundle_replacement_confirmation(
    kind: BundleKind,
    source: &Layout,
    target: &Layout,
    allow_replace: bool,
) -> Result<(), ManagerError> {
    if kind == BundleKind::Release {
        return Ok(());
    }
    require_replacement_confirmation(source, target, allow_replace)
}

pub(super) fn require_replacement_confirmation(
    source: &Layout,
    target: &Layout,
    allow_replace: bool,
) -> Result<(), ManagerError> {
    if allow_replace || !target.state.exists() {
        return Ok(());
    }
    let same_state = same_deployment_state(&source.state, &target.state)?;
    let same_subscriptions = same_file(&source.subscription_catalog, &target.subscription_catalog)?;
    if same_state && same_subscriptions {
        Ok(())
    } else {
        Err(ManagerError::ConfirmationRequired(format!(
            "{} already contains different state or subscriptions",
            target.home.display()
        )))
    }
}

fn same_deployment_state(left: &Path, right: &Path) -> Result<bool, ManagerError> {
    let mut left = read_document(left)?;
    let Ok(mut right) = read_document(right) else {
        return Ok(false);
    };
    left.runtime = Runtime::default();
    right.runtime = Runtime::default();
    left.updated_at = right.updated_at;
    Ok(left == right)
}

fn read_document(path: &Path) -> Result<Document, ManagerError> {
    let data = fs::read(path)
        .map_err(|error| ManagerError::io(format!("read {}", path.display()), error))?;
    serde_json::from_slice(&data).map_err(|error| {
        ManagerError::InvalidOperation(format!(
            "decode deployment state {}: {error}",
            path.display()
        ))
    })
}

fn same_file(left: &Path, right: &Path) -> Result<bool, ManagerError> {
    let left = match fs::read(left) {
        Ok(data) => Some(data),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(ManagerError::io(format!("read {}", left.display()), error)),
    };
    let right = match fs::read(right) {
        Ok(data) => Some(data),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(ManagerError::io(format!("read {}", right.display()), error)),
    };
    Ok(left == right)
}

pub(super) async fn restore_registration_after_rollback(target: &Layout, previous: State) {
    if previous == State::NotInstalled {
        let _ = sempre_service::uninstall().await;
    } else {
        let _ = sempre_service::install(&target.service_executable, &target.home).await;
        restore_service_state(previous).await;
    }
}

pub(super) async fn restore_service_state(previous: State) {
    if matches!(previous, State::Running | State::StartPending) {
        let _ = sempre_service::start().await;
    } else if matches!(previous, State::Stopped | State::StopPending) {
        let _ = sempre_service::stop().await;
    }
}

#[cfg(unix)]
pub(super) fn prepare_command_registration(layout: &Layout) -> Result<(), ManagerError> {
    match fs::read_link(&layout.command_executable) {
        Ok(target) if target == layout.service_executable => Ok(()),
        Ok(_) => Err(ManagerError::InvalidOperation(format!(
            "command path {} is already owned by another installation",
            layout.command_executable.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
            Err(ManagerError::InvalidOperation(format!(
                "command path {} exists and is not a symbolic link",
                layout.command_executable.display()
            )))
        }
        Err(error) => Err(ManagerError::io("inspect command registration", error)),
    }
}

#[cfg(unix)]
pub(super) fn register_command(layout: &Layout) -> Result<bool, ManagerError> {
    if fs::read_link(&layout.command_executable).is_ok() {
        return Ok(false);
    }
    let parent = layout.command_executable.parent().ok_or_else(|| {
        ManagerError::InvalidOperation("command registration has no parent".into())
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| ManagerError::io("create command directory", error))?;
    std::os::unix::fs::symlink(&layout.service_executable, &layout.command_executable)
        .map_err(|error| ManagerError::io("register system command", error))?;
    Ok(true)
}

#[cfg(unix)]
pub(super) fn unregister_command(layout: &Layout) -> Result<(), ManagerError> {
    match fs::read_link(&layout.command_executable) {
        Ok(target) if target == layout.service_executable => {
            fs::remove_file(&layout.command_executable)
                .map_err(|error| ManagerError::io("remove command registration", error))
        }
        Ok(_) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::InvalidInput
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(ManagerError::io("inspect command registration", error)),
    }
}

#[cfg(windows)]
#[allow(clippy::unnecessary_wraps)]
pub(super) fn prepare_command_registration(_: &Layout) -> Result<(), ManagerError> {
    Ok(())
}

#[cfg(windows)]
#[allow(clippy::unnecessary_wraps)]
pub(super) fn register_command(_: &Layout) -> Result<bool, ManagerError> {
    Ok(false)
}

#[cfg(windows)]
#[allow(clippy::unnecessary_wraps)]
pub(super) fn unregister_command(_: &Layout) -> Result<(), ManagerError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use super::*;

    #[cfg(unix)]
    #[test]
    fn command_unregistration_is_idempotent_and_preserves_foreign_paths() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let layout = Layout::system_at(&temporary.path().join("target"));
        fs::create_dir_all(
            layout
                .command_executable
                .parent()
                .expect("command directory"),
        )
        .expect("command directory");
        symlink(&layout.service_executable, &layout.command_executable).expect("owned command");
        unregister_command(&layout).expect("remove owned command");
        unregister_command(&layout).expect("missing command is already unregistered");
        assert!(!layout.command_executable.exists());

        let foreign = temporary.path().join("foreign-sempre");
        symlink(&foreign, &layout.command_executable).expect("foreign command");
        unregister_command(&layout).expect("preserve foreign command");
        assert_eq!(
            fs::read_link(&layout.command_executable).expect("foreign command remains"),
            foreign
        );
    }
}
