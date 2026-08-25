use std::{collections::BTreeSet, fs, path::Path};

use sempre_state::{Document, Layout, Runtime, write_atomic};

use crate::{BundleError, RestoreTransaction, copy_file, copy_tree};

use super::restore::{Swap, remove_path, unique_sibling};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeployComponent {
    All,
    Core,
    Bin,
    Data,
}

impl DeployComponent {
    pub const fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Core => "core",
            Self::Bin => "bin",
            Self::Data => "data",
        }
    }

    const fn includes_bin(self) -> bool {
        matches!(self, Self::All | Self::Bin)
    }

    const fn includes_core(self) -> bool {
        matches!(self, Self::All | Self::Core)
    }

    const fn includes_data(self) -> bool {
        matches!(self, Self::All | Self::Data)
    }
}

pub fn stage_deploy(
    source: &Layout,
    target: &Layout,
    component: DeployComponent,
    document: &Document,
) -> Result<RestoreTransaction, BundleError> {
    document.validate().map_err(|error| {
        BundleError::InvalidMetadata(format!("invalid deployment state: {error}"))
    })?;
    let mut transaction = RestoreTransaction::empty();
    let result = (|| {
        if component.includes_bin() {
            transaction.operations.push(Swap::stage(
                &source.service_executable,
                &target.service_executable,
                true,
                true,
            )?);
            transaction.operations.push(stage_merged_directory(
                &source.resources,
                &target.resources,
            )?);
            transaction
                .operations
                .push(stage_merged_directory(&source.tools, &target.tools)?);
        }
        if component.includes_core() {
            transaction
                .operations
                .push(if component == DeployComponent::Core {
                    stage_merged_directory(&source.cores, &target.cores)?
                } else {
                    Swap::stage(&source.cores, &target.cores, false, false)?
                });
        }
        if component.includes_data() {
            transaction
                .operations
                .push(stage_configurations(source, target, document)?);
            for (from, to, required) in [
                (&source.subscriptions, &target.subscriptions, false),
                (&source.gateway, &target.gateway, false),
                (&source.ui, &target.ui, false),
                (&source.tunnels, &target.tunnels, false),
                (&source.web_config, &target.web_config, true),
            ] {
                transaction
                    .operations
                    .push(Swap::stage(from, to, required, false)?);
            }
            transaction
                .operations
                .push(stage_document(&target.state, document)?);
        }
        Ok::<(), BundleError>(())
    })();
    result?;
    Ok(transaction)
}

fn stage_merged_directory(source: &Path, target: &Path) -> Result<Swap, BundleError> {
    let staged = unique_sibling(target, "stage");
    remove_path(&staged)?;
    fs::create_dir_all(&staged).map_err(|source_error| BundleError::Io {
        operation: "create merged deployment directory",
        path: staged.clone(),
        source: source_error,
    })?;
    if let Err(error) = copy_tree(target, &staged).and_then(|()| copy_tree(source, &staged)) {
        let _ = remove_path(&staged);
        return Err(error);
    }
    Ok(Swap::prepared(target, staged))
}

fn stage_configurations(
    source: &Layout,
    target: &Layout,
    document: &Document,
) -> Result<Swap, BundleError> {
    let staged = unique_sibling(&target.configs, "stage");
    remove_path(&staged)?;
    fs::create_dir_all(&staged).map_err(|source_error| BundleError::Io {
        operation: "create configuration deployment directory",
        path: staged.clone(),
        source: source_error,
    })?;
    let result = referenced_configurations(document)
        .into_iter()
        .try_for_each(|(core, hash)| {
            copy_file(
                &source.config(&core, &hash),
                &staged.join(core).join(format!("{hash}.json")),
                false,
            )
        });
    if let Err(error) = result {
        let _ = remove_path(&staged);
        return Err(error);
    }
    Ok(Swap::prepared(&target.configs, staged))
}

fn referenced_configurations(document: &Document) -> BTreeSet<(String, String)> {
    let mut result = document
        .configs
        .iter()
        .map(|(core, hash)| (core.clone(), hash.clone()))
        .collect::<BTreeSet<_>>();
    for deployment in [document.active.as_ref(), document.previous.as_ref()]
        .into_iter()
        .flatten()
    {
        result.insert((deployment.core.clone(), deployment.config_hash.clone()));
    }
    result
}

fn stage_document(target: &Path, document: &Document) -> Result<Swap, BundleError> {
    let staged = unique_sibling(target, "stage");
    let mut deployed = document.clone();
    deployed.runtime = Runtime::default();
    let mut data = serde_json::to_vec_pretty(&deployed).map_err(|source| BundleError::Encode {
        name: "deployment state",
        source,
    })?;
    data.push(b'\n');
    write_atomic(&staged, &data, 0o600).map_err(|source| BundleError::Io {
        operation: "write staged deployment state",
        path: staged.clone(),
        source,
    })?;
    Ok(Swap::prepared(target, staged))
}

#[cfg(test)]
mod tests {
    use sempre_state::{RuntimeState, Store};

    use super::*;

    #[test]
    fn components_replace_only_their_owned_boundaries_and_rollback() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = Layout::at(&temporary.path().join("source"));
        let target = Layout::system_at(&temporary.path().join("target"));
        target.ensure().expect("target layout");
        let store = Store::new(source.clone());
        store.initialize().expect("source state");
        let document = store.read().expect("source document");
        fs::write(&source.service_executable, b"new bin").expect("source bin");
        fs::create_dir_all(&source.cores).expect("source cores");
        fs::write(source.cores.join("new-core"), b"new core").expect("source core");
        fs::create_dir_all(&target.cores).expect("target cores");
        fs::write(target.cores.join("old-core"), b"old core").expect("target core");
        if let Some(parent) = target.service_executable.parent() {
            fs::create_dir_all(parent).expect("target bin parent");
        }
        fs::write(&target.service_executable, b"old bin").expect("target bin");

        let mut core =
            stage_deploy(&source, &target, DeployComponent::Core, &document).expect("stage core");
        core.activate().expect("activate core");
        assert_eq!(
            fs::read(&target.service_executable).expect("target bin"),
            b"old bin"
        );
        assert!(target.cores.join("old-core").is_file());
        assert!(target.cores.join("new-core").is_file());
        core.rollback();
        assert!(target.cores.join("old-core").is_file());
        assert!(!target.cores.join("new-core").exists());
    }

    #[test]
    fn data_deploy_keeps_only_referenced_configs_and_clears_runtime() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = Layout::at(&temporary.path().join("source"));
        let target = Layout::system_at(&temporary.path().join("target"));
        target.ensure().expect("target layout");
        let store = Store::new(source.clone());
        store.initialize().expect("source state");
        fs::write(
            &source.web_config,
            br#"{"schema":1,"listen":"127.0.0.1:1"}"#,
        )
        .expect("web config");
        let hash = "a".repeat(64);
        fs::create_dir_all(source.configs.join("sing-box")).expect("config directory");
        fs::write(source.config("sing-box", &hash), b"{}").expect("referenced config");
        fs::write(source.configs.join("sing-box/stale.json"), b"{}").expect("stale config");
        let mut document = store.read().expect("source document");
        document.configs.insert("sing-box".into(), hash.clone());
        document.runtime.state = RuntimeState::Running;
        let mut data =
            stage_deploy(&source, &target, DeployComponent::Data, &document).expect("stage data");
        data.activate().expect("activate data");
        data.commit().expect("commit data");
        assert!(target.config("sing-box", &hash).is_file());
        assert!(!target.configs.join("sing-box/stale.json").exists());
        let deployed = Store::new(target).read().expect("deployed state");
        assert_eq!(deployed.runtime, Runtime::default());
    }
}
