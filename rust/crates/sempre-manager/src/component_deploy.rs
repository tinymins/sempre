use sempre_bundle::{DeployComponent, RestoreTransaction};
use sempre_core::{Adapter, CoreRef};
use sempre_service::State;
use sempre_state::{Document, Layout, Mode, SourceState};

use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

use super::service_deploy::{
    prepare_command_registration, register_command, require_replacement_confirmation,
    restore_registration_after_rollback, restore_service_state, unregister_command,
};

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn deploy_component(
        &self,
        target: &Layout,
        component: DeployComponent,
        allow_replace: bool,
    ) -> Result<(), ManagerError> {
        let source = self.store.layout();
        if source.mode != Mode::Portable {
            return Err(ManagerError::InvalidOperation(
                "service deploy is only available in portable mode".into(),
            ));
        }
        if target.mode != Mode::System {
            return Err(ManagerError::InvalidOperation(
                "service deploy requires a system target".into(),
            ));
        }
        sempre_service::require_installation_privileges()?;
        let previous = sempre_service::status().await?;
        if previous == State::NotInstalled || !target.state.is_file() {
            return Err(ManagerError::InvalidOperation(
                "system deployment is not initialized; run 'sempre service install' first".into(),
            ));
        }
        let _operation = self.store.acquire_operation()?;
        let _configuration = if includes_data(component) {
            Some(self.store.acquire_config()?)
        } else {
            None
        };
        let document = self.store.read()?;
        if includes_core(component) {
            self.validate_deployed_cores(source, &document, "portable")
                .await?;
        }
        if component == DeployComponent::Data {
            self.validate_deployed_cores(target, &document, "system")
                .await?;
        }
        if includes_data(component) {
            self.validate_active_deployment(&document).await?;
            require_replacement_confirmation(source, target, allow_replace)?;
        }
        if includes_bin(component) {
            prepare_command_registration(target)?;
        }
        let transaction = sempre_bundle::stage_deploy(source, target, component, &document)?;
        activate_component(target, component, previous, transaction).await
    }

    async fn validate_active_deployment(&self, document: &Document) -> Result<(), ManagerError> {
        let Some(deployment) = &document.active else {
            return Ok(());
        };
        let reference = CoreRef {
            core: deployment.core.clone(),
            repository: deployment.repository.clone(),
            reference: deployment.reference.clone(),
        };
        let config = self
            .store
            .layout()
            .config(&deployment.core, &deployment.config_hash);
        self.validate_config_path(&reference, &deployment.version, &config)
            .await
    }

    async fn validate_deployed_cores(
        &self,
        layout: &Layout,
        document: &Document,
        owner: &'static str,
    ) -> Result<(), ManagerError> {
        for installed in installed_cores(document) {
            let adapter = self.registry.get(&installed.core)?;
            let binary = layout.core_binary(
                &installed.core,
                installed.repository.as_deref(),
                &installed.version,
            );
            validate_version(&self.runner, adapter.as_ref(), &binary, &installed, owner).await?;
        }
        Ok(())
    }
}

async fn activate_component(
    target: &Layout,
    component: DeployComponent,
    previous: State,
    mut transaction: RestoreTransaction,
) -> Result<(), ManagerError> {
    let repair_registration = includes_bin(component);
    if !matches!(previous, State::Stopped | State::NotInstalled)
        && let Err(error) = sempre_service::stop().await
    {
        restore_service_state(previous).await;
        return Err(error.into());
    }
    if let Err(error) = transaction.activate() {
        restore_service_state(previous).await;
        return Err(error.into());
    }
    if component == DeployComponent::All
        && let Err(error) = target.ensure()
    {
        return rollback(
            target,
            previous,
            repair_registration,
            false,
            &mut transaction,
            error.into(),
        )
        .await;
    }
    let mut command_created = false;
    if repair_registration {
        if let Err(error) = sempre_service::install(&target.service_executable, &target.home).await
        {
            return rollback(
                target,
                previous,
                true,
                false,
                &mut transaction,
                error.into(),
            )
            .await;
        }
        match register_command(target) {
            Ok(created) => command_created = created,
            Err(error) => {
                return rollback(target, previous, true, false, &mut transaction, error).await;
            }
        }
    }
    if matches!(previous, State::Running | State::StartPending)
        && let Err(error) = sempre_service::start().await
    {
        return rollback(
            target,
            previous,
            repair_registration,
            command_created,
            &mut transaction,
            error.into(),
        )
        .await;
    }
    transaction.commit()?;
    Ok(())
}

async fn rollback(
    target: &Layout,
    previous: State,
    repair_registration: bool,
    command_created: bool,
    transaction: &mut RestoreTransaction,
    cause: ManagerError,
) -> Result<(), ManagerError> {
    if command_created {
        let _ = unregister_command(target);
    }
    transaction.rollback();
    if repair_registration {
        restore_registration_after_rollback(target, previous).await;
    } else {
        restore_service_state(previous).await;
    }
    Err(cause)
}

async fn validate_version<R: VersionRunner>(
    runner: &R,
    adapter: &dyn Adapter,
    binary: &std::path::Path,
    installed: &InstalledCore,
    owner: &'static str,
) -> Result<(), ManagerError> {
    let actual = runner.version(adapter, binary).await.map_err(|error| {
        ManagerError::InvalidOperation(format!(
            "validate {owner} {}: {error}",
            installed.reference()
        ))
    })?;
    if actual == installed.version {
        Ok(())
    } else {
        Err(ManagerError::InvalidOperation(format!(
            "{owner} {} reports version {actual}, expected {}",
            installed.reference(),
            installed.version
        )))
    }
}

#[derive(Debug, Eq, PartialEq)]
struct InstalledCore {
    core: String,
    repository: Option<String>,
    version: String,
}

impl InstalledCore {
    fn reference(&self) -> String {
        self.repository.as_ref().map_or_else(
            || format!("{}@{}", self.core, self.version),
            |repository| format!("{}:{}@{}", self.core, repository, self.version),
        )
    }
}

fn installed_cores(document: &Document) -> Vec<InstalledCore> {
    let mut result = Vec::new();
    for (core, state) in &document.cores {
        append_installed(&mut result, core, None, &state.default);
        for (repository, source) in &state.custom {
            append_installed(&mut result, core, Some(repository), source);
        }
    }
    result
}

fn append_installed(
    result: &mut Vec<InstalledCore>,
    core: &str,
    repository: Option<&String>,
    source: &SourceState,
) {
    result.extend(source.installed.keys().map(|version| InstalledCore {
        core: core.into(),
        repository: repository.cloned(),
        version: version.clone(),
    }));
}

const fn includes_bin(component: DeployComponent) -> bool {
    matches!(component, DeployComponent::All | DeployComponent::Bin)
}

const fn includes_core(component: DeployComponent) -> bool {
    matches!(component, DeployComponent::All | DeployComponent::Core)
}

const fn includes_data(component: DeployComponent) -> bool {
    matches!(component, DeployComponent::All | DeployComponent::Data)
}

#[cfg(test)]
mod tests {
    use std::{future::Future, path::Path, pin::Pin};

    use chrono::Utc;
    use sempre_core::Adapter;
    use sempre_state::{Installation, Store};

    use super::*;

    #[derive(Clone, Copy)]
    struct FileVersionRunner;

    impl VersionRunner for FileVersionRunner {
        fn version<'a>(
            &'a self,
            _: &'a dyn Adapter,
            binary: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<String, ManagerError>> + Send + 'a>> {
            Box::pin(async move {
                std::fs::read_to_string(binary)
                    .map(|value| value.trim().to_owned())
                    .map_err(|error| ManagerError::io("read fake core version", error))
            })
        }
    }

    impl ValidationRunner for FileVersionRunner {
        fn validate<'a>(
            &'a self,
            _: &'a dyn Adapter,
            _: &'a Path,
            _: &'a Path,
            _: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<(), ManagerError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[test]
    fn installed_core_inventory_preserves_custom_repository_identity() {
        let mut document = Document::default();
        let installation = Installation {
            explicit: true,
            digest: format!("sha256:{}", "a".repeat(64)),
            source: "test".into(),
            installed_at: Utc::now(),
        };
        document
            .core_mut("sing-box")
            .default
            .installed
            .insert("1.2.3".into(), installation.clone());
        document
            .core_mut("mihomo")
            .custom
            .entry("owner/fork".into())
            .or_default()
            .installed
            .insert("2.0.0".into(), installation);
        assert_eq!(
            installed_cores(&document),
            vec![
                InstalledCore {
                    core: "mihomo".into(),
                    repository: Some("owner/fork".into()),
                    version: "2.0.0".into(),
                },
                InstalledCore {
                    core: "sing-box".into(),
                    repository: None,
                    version: "1.2.3".into(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn deployed_core_validation_probes_the_selected_layout_before_activation() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = Layout::at(&temporary.path().join("source"));
        let manager =
            Manager::with_runner(Store::new(source.clone()), FileVersionRunner).expect("manager");
        manager
            .store()
            .update(|document| {
                document.core_mut("sing-box").default.installed.insert(
                    "1.2.3".into(),
                    Installation {
                        explicit: true,
                        digest: format!("sha256:{}", "a".repeat(64)),
                        source: "test".into(),
                        installed_at: Utc::now(),
                    },
                );
                Ok(())
            })
            .expect("source state");
        let document = manager.state().expect("document");
        let target = Layout::system_at(&temporary.path().join("target"));
        let binary = target.core_binary("sing-box", None, "1.2.3");
        std::fs::create_dir_all(binary.parent().expect("core parent")).expect("core parent");
        std::fs::write(&binary, b"9.9.9\n").expect("wrong core version");

        let error = manager
            .validate_deployed_cores(&target, &document, "system")
            .await
            .expect_err("wrong system core version");
        assert!(
            error
                .to_string()
                .contains("reports version 9.9.9, expected 1.2.3")
        );
        std::fs::write(binary, b"1.2.3\n").expect("matching core version");
        manager
            .validate_deployed_cores(&target, &document, "system")
            .await
            .expect("matching system core version");
    }
}
