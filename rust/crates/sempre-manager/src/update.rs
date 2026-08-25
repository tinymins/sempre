use sempre_core::CoreRef;

use crate::{CoreChange, Manager, ManagerError, ValidationRunner, VersionRunner};

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn update_cores(&self, value: &str) -> Result<Vec<CoreChange>, ManagerError> {
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
            let result = self.install_core(&reference.to_string()).await?;
            changes.push(CoreChange {
                changed: result.changed,
                message: if result.changed {
                    format!("{}@{} installed", result.core, result.version)
                } else {
                    format!("{}@{} is already installed", result.core, result.version)
                },
                current_detail: format!("{} -> {}", result.reference, result.version),
                ..CoreChange::default()
            });
            if selected {
                let selection = self.select_core(&reference.to_string()).await?;
                if selection.changed {
                    changes.push(selection);
                }
            }
        }
        Ok(changes)
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

#[cfg(test)]
mod tests {
    use sempre_state::{Layout, Store};

    use super::*;

    #[tokio::test]
    async fn exact_versions_cannot_be_updated() {
        let root = tempfile::tempdir().expect("temporary directory");
        let manager = Manager::new(Store::new(Layout::at(root.path()))).expect("manager");
        let error = manager
            .update_cores("sing-box@1.12.20")
            .await
            .expect_err("exact update must fail");
        assert!(
            error
                .to_string()
                .contains("exact core versions are immutable")
        );
    }
}
