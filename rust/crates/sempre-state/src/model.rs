use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingConfigField {
    Sources,
    SubscriptionContent,
    Nodes,
    Groups,
    Rules,
    RuleProviders,
    Filters,
    Dns,
    PrivateAccess,
    LocalProxy,
    TransparentProxy,
    ManagementApi,
    Advanced,
    ManualConfiguration,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Document {
    pub schema: u32,
    pub updated_at: DateTime<Utc>,
    pub selected: Option<Selection>,
    pub active: Option<Deployment>,
    pub previous: Option<Deployment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_config_build: Option<ConfigBuild>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_profile_id: Option<String>,
    pub pending: bool,
    pub pending_config_fields: Vec<PendingConfigField>,
    pub last_error: Option<String>,
    pub cores: BTreeMap<String, CoreState>,
    pub configs: BTreeMap<String, String>,
    pub config_builds: BTreeMap<String, ConfigBuild>,
    pub subscription: Subscription,
    pub active_profile_id: Option<String>,
    pub subscription_auto_restart: bool,
    pub desired_state: DesiredState,
    pub runtime: Runtime,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesiredState {
    #[default]
    Running,
    Stopped,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Selection {
    pub core: String,
    pub repository: Option<String>,
    pub reference: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConfigBuild {
    pub profile_id: String,
    pub profile_revision: u64,
    pub target_key: String,
    pub runtime_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Deployment {
    pub core: String,
    pub repository: Option<String>,
    pub reference: String,
    pub version: String,
    pub config_hash: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CoreState {
    pub default: SourceState,
    pub custom: BTreeMap<String, SourceState>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceState {
    pub channels: BTreeMap<String, String>,
    pub installed: BTreeMap<String, Installation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Installation {
    pub explicit: bool,
    pub digest: String,
    pub source: String,
    pub installed_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Subscription {
    pub url: Option<String>,
    pub interval: String,
    pub last_check: Option<DateTime<Utc>>,
    pub last_change: Option<DateTime<Utc>>,
    pub last_result: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Runtime {
    pub state: RuntimeState,
    pub pid: Option<u32>,
    pub core: Option<String>,
    pub repository: Option<String>,
    pub reference: Option<String>,
    pub version: Option<String>,
    pub config_hash: Option<String>,
    pub runtime_config: Option<String>,
    pub runtime_config_hash: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub restart_count: u32,
    pub last_exit: Option<String>,
    pub last_error: Option<String>,
    pub last_failure: Option<RuntimeFailure>,
    pub last_transition: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    #[default]
    Idle,
    Starting,
    Running,
    Stopping,
    Restarting,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimeFailure {
    pub stage: String,
    pub error: String,
    pub occurred_at: DateTime<Utc>,
    pub failed: Option<Deployment>,
    pub rolled_back_to: Option<Deployment>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum StateValidationError {
    #[error("unsupported state schema {0}")]
    Schema(u32),
    #[error("invalid core ID {0:?}")]
    CoreId(String),
    #[error("invalid repository {0:?}")]
    Repository(String),
    #[error("invalid version {0:?}")]
    Version(String),
    #[error("invalid SHA-256 digest {0:?}")]
    Digest(String),
    #[error("core {0:?} is not installed")]
    MissingCore(String),
    #[error("core {core:?} repository {repository:?} is not installed")]
    MissingSource { core: String, repository: String },
    #[error("core {core:?} reference {reference:?} is unavailable")]
    MissingReference { core: String, reference: String },
    #[error("configuration build for {0:?} has no matching configuration")]
    MissingBuildConfiguration(String),
    #[error("configuration build for {0:?} is incomplete")]
    InvalidBuild(String),
    #[error("invalid subscription interval {0:?}")]
    SubscriptionInterval(String),
    #[error("invalid subscription URL")]
    SubscriptionUrl,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            updated_at: Utc::now(),
            selected: None,
            active: None,
            previous: None,
            previous_config_build: None,
            previous_profile_id: None,
            pending: false,
            pending_config_fields: Vec::new(),
            last_error: None,
            cores: BTreeMap::new(),
            configs: BTreeMap::new(),
            config_builds: BTreeMap::new(),
            subscription: Subscription::default(),
            active_profile_id: None,
            subscription_auto_restart: true,
            desired_state: DesiredState::Running,
            runtime: Runtime::default(),
        }
    }
}

impl Default for Subscription {
    fn default() -> Self {
        Self {
            url: None,
            interval: "24h".into(),
            last_check: None,
            last_change: None,
            last_result: None,
        }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            state: RuntimeState::Idle,
            pid: None,
            core: None,
            repository: None,
            reference: None,
            version: None,
            config_hash: None,
            runtime_config: None,
            runtime_config_hash: None,
            started_at: None,
            restart_count: 0,
            last_exit: None,
            last_error: None,
            last_failure: None,
            last_transition: None,
        }
    }
}

impl Document {
    pub fn validate(&self) -> Result<(), StateValidationError> {
        if self.schema != SCHEMA_VERSION {
            return Err(StateValidationError::Schema(self.schema));
        }
        for (core, state) in &self.cores {
            validate_core_id(core)?;
            validate_source(core, "", &state.default)?;
            for (repository, source) in &state.custom {
                validate_repository(repository)?;
                validate_source(core, repository, source)?;
            }
        }
        for (core, hash) in &self.configs {
            validate_core_id(core)?;
            validate_hash(hash)?;
        }
        for (core, build) in &self.config_builds {
            validate_core_id(core)?;
            if !self.configs.contains_key(core) {
                return Err(StateValidationError::MissingBuildConfiguration(
                    core.clone(),
                ));
            }
            if build.profile_id.trim().is_empty()
                || build.profile_revision == 0
                || build.target_key.trim().is_empty()
            {
                return Err(StateValidationError::InvalidBuild(core.clone()));
            }
        }
        if let Some(selection) = &self.selected {
            self.validate_selection(selection)?;
        }
        if let Some(deployment) = &self.active {
            self.validate_deployment(deployment)?;
        }
        if let Some(deployment) = &self.previous {
            self.validate_deployment(deployment)?;
        }
        validate_subscription(&self.subscription)?;
        if let Some(core) = &self.runtime.core {
            validate_core_id(core)?;
        }
        if let Some(repository) = &self.runtime.repository {
            validate_repository(repository)?;
        }
        if let Some(version) = &self.runtime.version {
            validate_version(version)?;
        }
        if let Some(reference) = &self.runtime.reference {
            validate_reference(reference)?;
        }
        if let Some(hash) = &self.runtime.config_hash {
            validate_hash(hash)?;
        }
        if let Some(hash) = &self.runtime.runtime_config_hash {
            validate_hash(hash)?;
        }
        Ok(())
    }

    pub fn core_mut(&mut self, core: &str) -> &mut CoreState {
        self.cores.entry(core.to_owned()).or_default()
    }

    pub fn stage(&mut self, deployment: Deployment) {
        if !self.pending {
            self.previous.clone_from(&self.active);
            self.previous_config_build = self
                .active
                .as_ref()
                .and_then(|active| self.config_builds.get(&active.core).cloned());
            self.previous_profile_id.clone_from(&self.active_profile_id);
        }
        self.active = Some(deployment);
        self.pending = true;
        self.last_error = None;
        self.runtime.last_failure = None;
    }

    fn validate_selection(&self, selection: &Selection) -> Result<(), StateValidationError> {
        validate_core_id(&selection.core)?;
        validate_reference(&selection.reference)?;
        let source = self.source(&selection.core, selection.repository.as_deref())?;
        if selection.reference == "stable" {
            if !source.channels.contains_key("stable") {
                return Err(StateValidationError::MissingReference {
                    core: selection.core.clone(),
                    reference: selection.reference.clone(),
                });
            }
        } else if !source.installed.contains_key(&selection.reference) {
            return Err(StateValidationError::MissingReference {
                core: selection.core.clone(),
                reference: selection.reference.clone(),
            });
        }
        Ok(())
    }

    fn validate_deployment(&self, deployment: &Deployment) -> Result<(), StateValidationError> {
        validate_core_id(&deployment.core)?;
        validate_reference(&deployment.reference)?;
        validate_version(&deployment.version)?;
        validate_hash(&deployment.config_hash)?;
        let source = self.source(&deployment.core, deployment.repository.as_deref())?;
        if !source.installed.contains_key(&deployment.version) {
            return Err(StateValidationError::MissingReference {
                core: deployment.core.clone(),
                reference: deployment.version.clone(),
            });
        }
        Ok(())
    }

    fn source(
        &self,
        core: &str,
        repository: Option<&str>,
    ) -> Result<&SourceState, StateValidationError> {
        let state = self
            .cores
            .get(core)
            .ok_or_else(|| StateValidationError::MissingCore(core.into()))?;
        match repository.filter(|value| !value.is_empty()) {
            Some(repository) => {
                state
                    .custom
                    .get(repository)
                    .ok_or_else(|| StateValidationError::MissingSource {
                        core: core.into(),
                        repository: repository.into(),
                    })
            }
            None => Ok(&state.default),
        }
    }
}

impl CoreState {
    pub fn source_mut(&mut self, repository: Option<&str>) -> &mut SourceState {
        match repository.filter(|value| !value.is_empty()) {
            Some(repository) => self.custom.entry(repository.to_owned()).or_default(),
            None => &mut self.default,
        }
    }
}

fn validate_source(
    core: &str,
    repository: &str,
    source: &SourceState,
) -> Result<(), StateValidationError> {
    for version in source.installed.keys() {
        validate_version(version)?;
    }
    for (channel, version) in &source.channels {
        if channel != "stable" || !source.installed.contains_key(version) {
            return Err(StateValidationError::MissingReference {
                core: core.into(),
                reference: version.clone(),
            });
        }
    }
    if !repository.is_empty() {
        validate_repository(repository)?;
    }
    Ok(())
}

fn validate_subscription(subscription: &Subscription) -> Result<(), StateValidationError> {
    if subscription.interval != "off" {
        let duration = humantime::parse_duration(&subscription.interval).map_err(|_| {
            StateValidationError::SubscriptionInterval(subscription.interval.clone())
        })?;
        if duration < std::time::Duration::from_mins(5) {
            return Err(StateValidationError::SubscriptionInterval(
                subscription.interval.clone(),
            ));
        }
    }
    if let Some(value) = subscription
        .url
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        let url = Url::parse(value).map_err(|_| StateValidationError::SubscriptionUrl)?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(StateValidationError::SubscriptionUrl);
        }
    }
    Ok(())
}

fn validate_core_id(value: &str) -> Result<(), StateValidationError> {
    let valid = !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value.as_bytes()[0].is_ascii_alphanumeric();
    if valid {
        Ok(())
    } else {
        Err(StateValidationError::CoreId(value.into()))
    }
}

fn validate_repository(value: &str) -> Result<(), StateValidationError> {
    let mut parts = value.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    let valid = parts.next().is_none()
        && !owner.is_empty()
        && !name.is_empty()
        && name != "."
        && name != ".."
        && owner.len() <= 39
        && name.len() <= 100
        && owner.as_bytes()[0].is_ascii_alphanumeric()
        && owner
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && name.bytes().all(repository_character);
    if valid {
        Ok(())
    } else {
        Err(StateValidationError::Repository(value.into()))
    }
}

fn repository_character(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
}

fn validate_reference(value: &str) -> Result<(), StateValidationError> {
    if value == "stable" {
        Ok(())
    } else {
        validate_version(value)
    }
}

fn validate_version(value: &str) -> Result<(), StateValidationError> {
    let suffix_index = value.find(['-', '+']);
    let (core, suffix) = suffix_index.map_or((value, None), |index| {
        (&value[..index], value.get(index + 1..))
    });
    let valid = core.split('.').count() == 3
        && core
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && suffix.is_none_or(|suffix| {
            !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        });
    if valid {
        Ok(())
    } else {
        Err(StateValidationError::Version(value.into()))
    }
}

fn validate_hash(value: &str) -> Result<(), StateValidationError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(StateValidationError::Digest(value.into()))
    }
}
