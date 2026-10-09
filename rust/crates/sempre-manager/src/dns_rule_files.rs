mod projection;
mod reuse;

use std::{
    fs,
    path::{Path, PathBuf},
};

use sempre_state::{Document, Layout, RuntimeState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{DnsSettings, ManagerError};

#[derive(Deserialize, Serialize)]
struct ArtifactSnapshot {
    settings: DnsSettings,
    trusted: bool,
}

#[derive(Clone, Deserialize, Serialize)]
struct RuntimeSnapshot {
    pid: u32,
    started_at: String,
    config_hash: String,
    settings: DnsSettings,
}

pub(crate) struct PreparedRuleFiles {
    config: PathBuf,
    settings: DnsSettings,
}

struct Replacement {
    path: PathBuf,
    previous: Vec<u8>,
    candidate: Vec<u8>,
}

pub(crate) struct RuleFileUpdate {
    files: Vec<Replacement>,
    snapshot_path: PathBuf,
    old_snapshot: Vec<u8>,
    snapshot: RuntimeSnapshot,
    validation_config: PathBuf,
    _validation_directory: tempfile::TempDir,
    published: usize,
    committed: bool,
}

pub(crate) fn save_snapshot(
    artifact: &Path,
    content: &str,
    settings: &DnsSettings,
) -> Result<(), ManagerError> {
    let document: Value = decode(content.as_bytes())?;
    write(
        &artifact.with_extension("dns-rule-snapshot.json"),
        &encode(&ArtifactSnapshot {
            settings: settings.clone(),
            trusted: projection::matches_inline(&document, settings),
        })?,
    )
}

pub(crate) fn prepare(
    config: &Path,
    artifact: &Path,
    saved: &DnsSettings,
) -> Result<Option<PreparedRuleFiles>, ManagerError> {
    let mut document: Value = decode(&read(config)?)?;
    let snapshot = match fs::read(artifact.with_extension("dns-rule-snapshot.json")) {
        Ok(bytes) => decode::<ArtifactSnapshot>(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ManagerError::io("read DNS rule artifact snapshot", error)),
    };
    if !snapshot.trusted || !projection::matches_inline(&document, &snapshot.settings) {
        return Ok(None);
    }
    let settings = if projection::same_structure(&snapshot.settings, saved) {
        saved.clone()
    } else {
        snapshot.settings
    };
    if settings.rule_sets.is_empty() {
        return Ok(None);
    }
    let definitions = document["route"]["rule_set"]
        .as_array_mut()
        .expect("validated definitions");
    for rule_set in &settings.rule_sets {
        let path = projection::file_path(config, rule_set);
        write(&path, &projection::source(rule_set)?)?;
        let definition = definitions
            .iter_mut()
            .find(|value| value["tag"] == projection::tag(rule_set))
            .expect("validated definition");
        *definition = projection::local(rule_set, &path);
    }
    write(config, &encode(&document)?)?;
    Ok(Some(PreparedRuleFiles {
        config: config.into(),
        settings,
    }))
}

pub(crate) fn record_started(
    prepared: &PreparedRuleFiles,
    document: &Document,
) -> Result<(), ManagerError> {
    let runtime = &document.runtime;
    let snapshot = RuntimeSnapshot {
        pid: runtime
            .pid
            .ok_or_else(|| invalid("DNS rule runtime PID is unavailable"))?,
        started_at: runtime
            .started_at
            .ok_or_else(|| invalid("DNS rule runtime start time is unavailable"))?
            .to_rfc3339(),
        config_hash: runtime
            .runtime_config_hash
            .clone()
            .ok_or_else(|| invalid("DNS rule runtime configuration hash is unavailable"))?,
        settings: prepared.settings.clone(),
    };
    write(&state_path(&prepared.config), &encode(&snapshot)?)
}

pub(crate) fn legacy_inline(document: &Document, previous: &DnsSettings) -> bool {
    let runtime = &document.runtime;
    if runtime.state != RuntimeState::Running
        || runtime.core.as_deref() != Some("sing-box")
        || previous.rule_sets.is_empty()
    {
        return false;
    }
    runtime.runtime_config.as_deref().is_some_and(|path| {
        fs::read(path)
            .ok()
            .and_then(|data| serde_json::from_slice::<Value>(&data).ok())
            .is_some_and(|document| projection::matches_inline(&document, previous))
    })
}

pub(crate) fn prepare_update(
    layout: &Layout,
    document: &Document,
    previous: &DnsSettings,
    candidate: &DnsSettings,
) -> Result<Option<RuleFileUpdate>, ManagerError> {
    let runtime = &document.runtime;
    if runtime.state != RuntimeState::Running || runtime.core.as_deref() != Some("sing-box") {
        return Ok(None);
    }
    let Some(config) = runtime.runtime_config.as_deref().map(Path::new) else {
        return Ok(None);
    };
    let snapshot_path = state_path(config);
    let old_snapshot = match fs::read(&snapshot_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ManagerError::io("read running DNS rule snapshot", error)),
    };
    let mut snapshot: RuntimeSnapshot = decode(&old_snapshot)?;
    if runtime.pid != Some(snapshot.pid)
        || !crate::runtime::process_alive(snapshot.pid)
        || runtime.started_at.map(|time| time.to_rfc3339()).as_deref() != Some(&snapshot.started_at)
        || runtime.runtime_config_hash.as_deref() != Some(&snapshot.config_hash)
        || !projection::same_structure(&snapshot.settings, previous)
        || !projection::same_structure(&snapshot.settings, candidate)
    {
        return Ok(None);
    }
    let data = read(config)?;
    if format!("{:x}", Sha256::digest(&data)) != snapshot.config_hash {
        return Ok(None);
    }
    let mut validation: Value = decode(&data)?;
    let Some(definitions) = validation["route"]["rule_set"].as_array_mut() else {
        return Ok(None);
    };
    let directory = tempfile::Builder::new()
        .prefix("dns-rule-update-")
        .tempdir_in(&layout.runtime)
        .map_err(|error| ManagerError::io("create DNS rule validation directory", error))?;
    let mut files = Vec::new();
    for (rule_set, applied) in candidate.rule_sets.iter().zip(&snapshot.settings.rule_sets) {
        let path = projection::file_path(config, rule_set);
        let matching = definitions
            .iter()
            .enumerate()
            .filter(|(_, value)| value["tag"] == projection::tag(rule_set))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let [index] = matching.as_slice() else {
            return Ok(None);
        };
        if definitions[*index] != projection::local(rule_set, &path) {
            return Ok(None);
        }
        let old_data = read(&path)?;
        if decode::<Value>(&old_data)? != decode::<Value>(&projection::source(applied)?)? {
            return Ok(None);
        }
        let candidate_data = projection::source(rule_set)?;
        let validation_path = directory.path().join(format!("{}.json", rule_set.id));
        write(&validation_path, &candidate_data)?;
        definitions[*index] = projection::local(rule_set, &validation_path);
        if old_data != candidate_data {
            files.push(Replacement {
                path,
                previous: old_data,
                candidate: candidate_data,
            });
        }
    }
    let validation_config = directory.path().join("config.json");
    write(&validation_config, &encode(&validation)?)?;
    snapshot.settings = candidate.clone();
    Ok(Some(RuleFileUpdate {
        files,
        snapshot_path,
        old_snapshot,
        snapshot,
        validation_config,
        _validation_directory: directory,
        published: 0,
        committed: false,
    }))
}

impl RuleFileUpdate {
    pub(crate) fn validation_config(&self) -> &Path {
        &self.validation_config
    }

    pub(crate) fn publish(&mut self) -> Result<(), ManagerError> {
        if !crate::runtime::process_alive(self.snapshot.pid) {
            return Err(invalid("sing-box exited before publishing DNS rules"));
        }
        for index in 0..self.files.len() {
            let replacement = &self.files[index];
            if read(&replacement.path)? != replacement.previous {
                return Err(invalid("DNS rule file changed during update"));
            }
            // Include this file in rollback even if persistence fails after its rename.
            self.published = index + 1;
            write(&replacement.path, &replacement.candidate)?;
        }
        Ok(())
    }

    pub(crate) fn commit(&mut self) -> Result<(), ManagerError> {
        if !crate::runtime::process_alive(self.snapshot.pid) {
            return Err(invalid("sing-box exited while publishing DNS rules"));
        }
        self.committed = true;
        write(&self.snapshot_path, &encode(&self.snapshot)?)
    }

    pub(crate) fn rollback(&mut self) -> Result<(), ManagerError> {
        let mut failures = Vec::new();
        for replacement in self.files.iter().take(self.published).rev() {
            if let Err(error) = write(&replacement.path, &replacement.previous) {
                failures.push(error.to_string());
            }
        }
        if self.committed
            && let Err(error) = write(&self.snapshot_path, &self.old_snapshot)
        {
            failures.push(error.to_string());
        }
        if failures.is_empty() {
            self.published = 0;
            self.committed = false;
            Ok(())
        } else {
            Err(invalid(&format!(
                "DNS rule rollback failed: {}",
                failures.join("; ")
            )))
        }
    }
}

fn state_path(config: &Path) -> PathBuf {
    config.with_extension("dns-rule-state.json")
}

fn invalid(message: &str) -> ManagerError {
    ManagerError::InvalidOperation(message.into())
}

fn read(path: &Path) -> Result<Vec<u8>, ManagerError> {
    fs::read(path).map_err(|error| ManagerError::io("read DNS rule runtime file", error))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), ManagerError> {
    sempre_state::write_atomic(path, bytes, 0o600)
        .map_err(|error| ManagerError::io("publish DNS rule runtime file", error))
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, ManagerError> {
    serde_json::from_slice(bytes)
        .map_err(|error| invalid(&format!("decode DNS rule runtime metadata: {error}")))
}

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, ManagerError> {
    serde_json::to_vec_pretty(value)
        .map_err(|error| invalid(&format!("encode DNS rule runtime metadata: {error}")))
}
