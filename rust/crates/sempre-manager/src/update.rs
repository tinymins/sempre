use sempre_core::CoreRef;

use crate::{
    CoreChange, Manager, ManagerError, ValidationRunner, VersionRunner,
    install::{CoreInstallProgress, InstallResult},
};

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn update_cores(&self, value: &str) -> Result<Vec<CoreChange>, ManagerError> {
        self.update_cores_observed(value, &|_| {}).await
    }

    pub(crate) async fn update_cores_observed(
        &self,
        value: &str,
        progress: &(impl Fn(CoreInstallProgress) + Sync),
    ) -> Result<Vec<CoreChange>, ManagerError> {
        // The stable channel follows the package or firmware there; versions are downloaded by name.
        if self.store.layout().openwrt {
            return self.update_router_core(value, progress).await;
        }
        let references = if value.trim().is_empty() {
            installed_channels(&self.store.read()?)
        } else {
            let reference = self.normalized_reference(value)?;
            if !reference.is_channel() {
                return Err(ManagerError::InvalidOperation(format!(
                    "exact core versions are immutable; update a channel such as {}@stable",
                    reference.core
                )));
            }
            vec![reference]
        };
        if references.is_empty() {
            return Err(ManagerError::InvalidOperation(
                "no core channels are installed".into(),
            ));
        }
        let mut changes = Vec::new();
        for reference in references {
            let selected = selection_matches(&self.store.read()?, &reference);
            let result = self
                .install_core_observed(&reference.to_string(), progress)
                .await?;
            changes.push(installed(&result));
            if selected {
                let selection = self.select_core(&reference.to_string()).await?;
                if selection.changed {
                    changes.push(selection);
                }
            }
        }
        Ok(changes)
    }

    /// Installs the latest stable release as an exact version and switches to it when selected.
    async fn update_router_core(
        &self,
        value: &str,
        progress: &(impl Fn(CoreInstallProgress) + Sync),
    ) -> Result<Vec<CoreChange>, ManagerError> {
        let core = if value.trim().is_empty() {
            String::from("sing-box")
        } else {
            self.normalized_reference(value)?.core
        };
        self.reject_firmware_managed_core(&core)?;
        let adapter = self.registry.get(&core)?;
        let stable = CoreRef::parse(&format!("{core}@stable"))?;
        let package = self.resolve_package(adapter.as_ref(), &stable).await?;
        let exact = format!("{core}@{}", package.version);
        let selected = self
            .store
            .read()?
            .selected
            .is_some_and(|selected| selected.core == core);
        let result = self.install_core_observed(&exact, progress).await?;
        let mut changes = vec![installed(&result)];
        if selected {
            let selection = self.select_core(&exact).await?;
            if selection.changed {
                changes.push(selection);
            }
        }
        Ok(changes)
    }
}

fn installed(result: &InstallResult) -> CoreChange {
    CoreChange {
        changed: result.changed,
        message: if result.changed {
            format!("{}@{} installed", result.core, result.version)
        } else {
            format!("{}@{} is already installed", result.core, result.version)
        },
        current_detail: format!("{} -> {}", result.reference, result.version),
        ..CoreChange::default()
    }
}

fn installed_channels(document: &sempre_state::Document) -> Vec<CoreRef> {
    let mut references = Vec::new();
    for (core, state) in &document.cores {
        for channel in state.default.channels.keys() {
            references.push(CoreRef {
                core: core.clone(),
                repository: None,
                reference: channel.clone(),
            });
        }
        for (repository, source) in &state.custom {
            for channel in source.channels.keys() {
                references.push(CoreRef {
                    core: core.clone(),
                    repository: Some(repository.clone()),
                    reference: channel.clone(),
                });
            }
        }
    }
    references.sort_by_key(ToString::to_string);
    references
}

fn selection_matches(document: &sempre_state::Document, reference: &CoreRef) -> bool {
    document.selected.as_ref().is_some_and(|selected| {
        selected.core == reference.core
            && selected.repository == reference.repository
            && selected.reference == reference.reference
    })
}
