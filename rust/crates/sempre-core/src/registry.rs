use std::{collections::BTreeMap, path::Path, sync::Arc};

use thiserror::Error;

use crate::{
    AssetSelection, Capabilities, CommandSpec, CompilerTarget, Definition, RunSpec, RuntimeSpec,
    Stability, Target,
};

pub trait Adapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn default_repository(&self) -> &'static str;
    fn definition(&self) -> Definition;
    fn capabilities(&self, version: Option<&str>, target: &Target) -> Capabilities;
    fn executable_name(&self, target: &Target) -> Result<String, RegistryError>;
    fn package_assets(
        &self,
        version: &str,
        target: &Target,
    ) -> Result<AssetSelection, RegistryError>;
    fn version_command(&self, binary: &str) -> CommandSpec;
    fn parse_version(&self, output: &str) -> Result<String, RegistryError>;
    fn compiler_target(
        &self,
        version: Option<&str>,
        target: &Target,
    ) -> Result<CompilerTarget, RegistryError>;
    fn validation_command(&self, binary: &str, config: &str, data_directory: &str) -> CommandSpec;
    fn run_spec(&self, binary: &str, config: &str, data_directory: &str) -> RunSpec;
    fn prepare_runtime(
        &self,
        config: &Path,
        runtime_directory: &Path,
    ) -> Result<RuntimeSpec, RegistryError>;
}

#[derive(Default)]
pub struct Registry {
    adapters: BTreeMap<String, Arc<dyn Adapter>>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RegistryError {
    #[error("core {0:?} is not supported")]
    Unsupported(String),
    #[error("core {core} does not support target {target}")]
    Target { core: String, target: String },
    #[error("{core} returned unrecognized version output {output:?}")]
    VersionOutput { core: String, output: String },
    #[error("prepare {core} runtime: {message}")]
    Runtime { core: String, message: String },
}

impl Registry {
    pub fn new(adapters: impl IntoIterator<Item = Arc<dyn Adapter>>) -> Self {
        let mut registry = Self::default();
        for adapter in adapters {
            registry.adapters.insert(adapter.id().into(), adapter);
        }
        registry
    }

    pub fn get(&self, id: &str) -> Result<Arc<dyn Adapter>, RegistryError> {
        self.adapters
            .get(id)
            .cloned()
            .ok_or_else(|| RegistryError::Unsupported(id.into()))
    }

    pub fn ids(&self) -> Vec<String> {
        self.adapters.keys().cloned().collect()
    }

    pub fn definitions(&self) -> Vec<Definition> {
        self.adapters
            .values()
            .map(|adapter| {
                let mut definition = adapter.definition();
                definition.id = adapter.id().into();
                definition.platforms.sort();
                definition.platforms.dedup();
                definition
            })
            .collect()
    }

    pub fn stable_capabilities(&self, target: &Target) -> Capabilities {
        Capabilities::intersection(
            self.adapters
                .values()
                .filter(|adapter| adapter.definition().stability == Stability::Stable)
                .map(|adapter| adapter.capabilities(None, target)),
        )
    }
}
