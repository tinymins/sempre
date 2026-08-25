use sempre_core::{CoreRef, Definition};
use sempre_state::{Deployment, Installation, Selection};
use serde::Serialize;

use crate::{Manager, ManagerError, VersionRunner};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InstalledCore {
    pub core: String,
    pub repository: String,
    pub reference: String,
    pub official: bool,
    pub version: String,
    pub channels: Vec<String>,
    pub installation: Installation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CoreInventory {
    pub supported: Vec<String>,
    pub catalog: Vec<Definition>,
    pub installed: Vec<InstalledCore>,
    pub selected: Option<Selection>,
    pub active: Option<Deployment>,
}

impl<R: VersionRunner> Manager<R> {
    pub fn core_inventory(&self) -> Result<CoreInventory, ManagerError> {
        let document = self.store.read()?;
        let mut installed = Vec::new();
        for (core, state) in &document.cores {
            let adapter = self.registry.get(core)?;
            collect_source(
                &mut installed,
                core,
                None,
                adapter.default_repository(),
                &state.default,
            );
            for (repository, source) in &state.custom {
                collect_source(&mut installed, core, Some(repository), repository, source);
            }
        }
        installed.sort_by(|left, right| {
            (&left.core, &left.repository, &left.version).cmp(&(
                &right.core,
                &right.repository,
                &right.version,
            ))
        });
        Ok(CoreInventory {
            supported: self.registry.ids(),
            catalog: self.registry.definitions(),
            installed,
            selected: document.selected,
            active: document.active,
        })
    }
}

fn collect_source(
    output: &mut Vec<InstalledCore>,
    core: &str,
    repository: Option<&String>,
    display_repository: &str,
    source: &sempre_state::SourceState,
) {
    for (version, installation) in &source.installed {
        let mut channels: Vec<String> = source
            .channels
            .iter()
            .filter(|(_, target)| *target == version)
            .map(|(channel, _)| channel.clone())
            .collect();
        channels.sort();
        output.push(InstalledCore {
            core: core.into(),
            repository: display_repository.into(),
            reference: CoreRef {
                core: core.into(),
                repository: repository.cloned(),
                reference: version.clone(),
            }
            .to_string(),
            official: repository.is_none(),
            version: version.clone(),
            channels,
            installation: installation.clone(),
        });
    }
}
