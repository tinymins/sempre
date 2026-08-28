use std::{collections::BTreeMap, path::Path, sync::Arc};

use thiserror::Error;

use crate::{
    AssetSelection, AutoConfigCandidate, AutoConfigCandidateProfile, AutoConfigRequirements,
    Capabilities, CommandSpec, CompilerTarget, Definition, RunSpec, RuntimeSpec, Stability, Target,
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
    fn auto_config_profiles(&self, _target: &Target) -> Vec<AutoConfigCandidateProfile> {
        Vec::new()
    }
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
    #[error("invalid automatic configuration recommendation {0:?}")]
    InvalidRecommendation(String),
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

    pub fn auto_config_candidates(
        &self,
        target: &Target,
        requirements: &AutoConfigRequirements,
    ) -> Result<Vec<AutoConfigCandidate>, RegistryError> {
        let mut candidates = Vec::new();
        let mut assessed = Vec::new();
        let mut identifiers = std::collections::BTreeSet::new();
        for adapter in self.adapters.values() {
            for profile in adapter.auto_config_profiles(target) {
                if profile.id.is_empty() || !identifiers.insert(profile.id.clone()) {
                    return Err(RegistryError::InvalidRecommendation(profile.id));
                }
                let reference = crate::CoreRef::parse(&profile.reference)
                    .map_err(|_| RegistryError::InvalidRecommendation(profile.id.clone()))?;
                if reference.core != adapter.id() {
                    return Err(RegistryError::InvalidRecommendation(profile.id));
                }
                let version = (!reference.is_channel()).then_some(reference.reference.as_str());
                let capabilities = adapter.capabilities(version, target);
                assessed.push((profile, adapter.id(), capabilities));
            }
        }
        for (profile, core, capabilities) in assessed {
            candidates.push(crate::assessment::evaluate(
                profile,
                core,
                &capabilities,
                requirements,
            ));
        }
        candidates.sort_by(|left, right| {
            right
                .eligible
                .cmp(&left.eligible)
                .then_with(|| right.score.cmp(&left.score))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(candidates)
    }
}
