use std::{fs, path::Path};

use sempre_manager::Manager;
use sempre_state::{Layout, Mode, RuntimeState, Store};
use serde::Serialize;
use sha2::{Digest as _, Sha256};

use crate::ClientError;

#[derive(Serialize)]
struct Report {
    checks: Vec<Check>,
    failures: usize,
    warnings: usize,
}

#[derive(Serialize)]
struct Check {
    level: Level,
    name: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum Level {
    Required,
    Warning,
    Info,
}

impl Report {
    fn new() -> Self {
        Self {
            checks: Vec::new(),
            failures: 0,
            warnings: 0,
        }
    }

    fn required(&mut self, name: &str, result: Result<String, String>) {
        self.push(Level::Required, name, result);
    }

    fn warning(&mut self, name: &str, result: Result<String, String>) {
        self.push(Level::Warning, name, result);
    }

    fn info(&mut self, name: &str, detail: impl Into<String>) {
        self.checks.push(Check {
            level: Level::Info,
            name: name.into(),
            ok: true,
            detail: Some(detail.into()),
        });
    }

    fn push(&mut self, level: Level, name: &str, result: Result<String, String>) {
        let (ok, detail) = match result {
            Ok(detail) => (true, (!detail.is_empty()).then_some(detail)),
            Err(detail) => {
                match level {
                    Level::Required => self.failures += 1,
                    Level::Warning => self.warnings += 1,
                    Level::Info => {}
                }
                (false, Some(detail))
            }
        };
        self.checks.push(Check {
            level,
            name: name.into(),
            ok,
            detail,
        });
    }

    fn print(&self, json: bool) -> Result<(), ClientError> {
        if json {
            println!("{}", serde_json::to_string_pretty(self)?);
            return Ok(());
        }
        for check in &self.checks {
            let label = match (check.level, check.ok) {
                (Level::Required | Level::Warning, true) => " OK ",
                (Level::Required, false) => "FAIL",
                (Level::Warning, false) => "WARN",
                (Level::Info, _) => "INFO",
            };
            match &check.detail {
                Some(detail) => println!("[{label}] {}: {detail}", check.name),
                None => println!("[{label}] {}", check.name),
            }
        }
        if self.failures == 0 {
            if self.warnings == 0 {
                println!("All checks passed.");
            } else {
                println!(
                    "All required checks passed with {} warning(s).",
                    self.warnings
                );
            }
        }
        Ok(())
    }
}

pub(crate) async fn run(mode: Mode, json: bool) -> Result<(), ClientError> {
    let layout = Layout::for_mode(mode)?;
    let manager = Manager::new(Store::new(layout.clone()))?;
    let state = manager.state()?;
    let mut report = Report::new();

    report.required("data directory", writable_directory(&layout.home));
    if mode == Mode::System {
        report.required("data protection", protected_directory(&layout.home));
    }
    service_checks(&mut report, mode, &layout).await;
    core_checks(&mut report, &manager, &layout, &state).await;

    let runtime = manager.runtime_status()?;
    runtime_state_check(&mut report, &runtime);
    let catalog = manager.subscriptions().read()?;
    let profile = state
        .active_profile_id
        .as_deref()
        .and_then(|id| catalog.profiles.iter().find(|profile| profile.id == id));
    profile_checks(&mut report, &manager, &layout, &state, &runtime, profile).await;

    report.print(json)?;
    if report.failures == 0 {
        Ok(())
    } else {
        Err(ClientError::Doctor(report.failures))
    }
}

async fn service_checks(report: &mut Report, mode: Mode, layout: &Layout) {
    match sempre_service::status().await {
        Ok(status) => {
            report.required("service manager", Ok(status.to_string()));
            if mode == Mode::System {
                if status == sempre_service::State::NotInstalled {
                    report.info("system service", "not installed");
                } else {
                    report.required(
                        "service executable",
                        regular_file(&layout.service_executable),
                    );
                    report.required("command registration", command_registration(layout));
                }
            }
        }
        Err(error) => report.required("service manager", Err(error.to_string())),
    }
}

async fn core_checks(
    report: &mut Report,
    manager: &Manager,
    layout: &Layout,
    state: &sempre_state::Document,
) {
    let active = state.active.as_ref();
    match active {
        None => report.required("active core", Err("not selected".into())),
        Some(active) => {
            let binary =
                layout.core_binary(&active.core, active.repository.as_deref(), &active.version);
            report.required("active core binary", regular_file(&binary));
            let config = layout.config(&active.core, &active.config_hash);
            report.required("active configuration", regular_file(&config));
            report.required(
                "active configuration hash",
                file_hash(&config, &active.config_hash),
            );
        }
    }

    match manager.current_config() {
        Ok(config) => report.required(
            "configuration validation",
            manager
                .validate_config_content(config.content.as_bytes())
                .await
                .map(|()| String::new())
                .map_err(|error| error.to_string()),
        ),
        Err(error) => report.required("configuration validation", Err(error.to_string())),
    }
}

fn runtime_state_check(report: &mut Report, runtime: &sempre_manager::RuntimeStatus) {
    if runtime.runtime_state == RuntimeState::Failed {
        report.required(
            "runtime state",
            Err(runtime
                .last_error
                .clone()
                .unwrap_or_else(|| "managed runtime failed".into())),
        );
    } else {
        report.required(
            "runtime state",
            Ok(format!("{:?}", runtime.runtime_state).to_lowercase()),
        );
    }
}

async fn profile_checks(
    report: &mut Report,
    manager: &Manager,
    layout: &Layout,
    state: &sempre_state::Document,
    runtime: &sempre_manager::RuntimeStatus,
    profile: Option<&sempre_converter::Profile>,
) {
    match (state.active_profile_id.as_deref(), profile) {
        (Some(_), Some(profile)) => {
            report.required("active subscription profile", Ok(profile.name.clone()));
        }
        (Some(_), None) => report.required(
            "active subscription profile",
            Err("active profile is missing".into()),
        ),
        (None, _) => report.info("active subscription profile", "not configured"),
    }

    if let (Some(active), Some(profile)) = (state.active.as_ref(), profile) {
        match state.config_builds.get(&active.core) {
            Some(build)
                if build.profile_id == profile.id && build.profile_revision == profile.revision =>
            {
                report.required(
                    "profile configuration revision",
                    Ok(format!("revision {}", profile.revision)),
                );
            }
            Some(build) => report.required(
                "profile configuration revision",
                Err(format!(
                    "active build uses profile {:?} revision {}, expected {:?} revision {}",
                    build.profile_id, build.profile_revision, profile.id, profile.revision
                )),
            ),
            None => report.info(
                "profile configuration revision",
                "not recorded for the initial imported configuration",
            ),
        }
        running_checks(report, layout, state, active);
        management_check(report, manager, layout, runtime).await;
        transparent_network_checks(report, profile).await;
    }
}

fn running_checks(
    report: &mut Report,
    _layout: &Layout,
    state: &sempre_state::Document,
    active: &sempre_state::Deployment,
) {
    if state.runtime.state != RuntimeState::Running {
        report.info("runtime configuration hashes", "runtime is not running");
        return;
    }
    report.required(
        "runtime source configuration hash",
        (state.runtime.config_hash.as_deref() == Some(active.config_hash.as_str()))
            .then_some(String::new())
            .ok_or_else(|| "runtime source hash does not match the active deployment".into()),
    );
    match (
        state.runtime.runtime_config.as_deref(),
        state.runtime.runtime_config_hash.as_deref(),
    ) {
        (Some(path), Some(hash)) => report.required(
            "prepared runtime configuration hash",
            file_hash(Path::new(path), hash),
        ),
        _ => report.required(
            "prepared runtime configuration hash",
            Err("runtime configuration metadata is missing".into()),
        ),
    }
}

async fn management_check(
    report: &mut Report,
    manager: &Manager,
    layout: &Layout,
    runtime: &sempre_manager::RuntimeStatus,
) {
    if runtime.runtime_state != RuntimeState::Running {
        return;
    }
    let supported = manager.configuration_context().is_ok_and(|context| {
        context
            .capabilities
            .features
            .iter()
            .any(|item| item == "management.external_api")
    });
    if !supported {
        return;
    }
    let result = match sempre_core_control::Client::from_file(&layout.core_control) {
        Ok(client) => client.overview().await.map(|overview| overview.version),
        Err(error) => Err(error),
    };
    report.required(
        "external management API",
        result.map_err(|error| error.to_string()),
    );
}

async fn transparent_network_checks(report: &mut Report, profile: &sempre_converter::Profile) {
    let transparent = &profile.transparent_proxy;
    let host_capture =
        transparent.mode == "tun" || (transparent.mode == "tproxy" && transparent.capture_host);
    if host_capture {
        match sempre_network::run_network_test().await {
            Ok(network) => {
                for result in network.results {
                    report.warning(
                        &format!("network {}", result.name),
                        if result.ok {
                            Ok(format!("{} ms", result.latency_ms))
                        } else {
                            Err(result.detail.unwrap_or_else(|| "probe failed".into()))
                        },
                    );
                }
            }
            Err(error) => report.warning("transparent network probes", Err(error.to_string())),
        }
    } else if transparent.mode == "tproxy" {
        report.warning(
            "LAN transparent traffic probe",
            Err("a live LAN client is required because host capture is disabled".into()),
        );
    }
}

fn writable_directory(path: &Path) -> Result<String, String> {
    tempfile::Builder::new()
        .prefix(".sempre-doctor-")
        .tempfile_in(path)
        .map(|_| path.display().to_string())
        .map_err(|error| error.to_string())
}

fn regular_file(path: &Path) -> Result<String, String> {
    path.is_file()
        .then(|| path.display().to_string())
        .ok_or_else(|| format!("{} is not a regular file", path.display()))
}

fn file_hash(path: &Path, expected: &str) -> Result<String, String> {
    let data = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let actual = format!("{:x}", Sha256::digest(data));
    (actual == expected)
        .then_some(actual.clone())
        .ok_or_else(|| format!("SHA-256 {actual} does not match {expected}"))
}

#[cfg(unix)]
fn protected_directory(path: &Path) -> Result<String, String> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let mode = metadata.permissions().mode() & 0o777;
    if metadata.uid() == 0 && mode.is_multiple_of(0o100) {
        Ok(format!("root-owned mode {mode:o}"))
    } else {
        Err(format!(
            "expected root ownership and private mode, found uid {} mode {mode:o}",
            metadata.uid()
        ))
    }
}

#[cfg(windows)]
fn protected_directory(path: &Path) -> Result<String, String> {
    fs::metadata(path)
        .map(|_| path.display().to_string())
        .map_err(|error| error.to_string())
}

#[cfg(unix)]
fn command_registration(layout: &Layout) -> Result<String, String> {
    let target = fs::read_link(&layout.command_executable).map_err(|error| error.to_string())?;
    if target == layout.service_executable {
        Ok(layout.command_executable.display().to_string())
    } else {
        Err(format!(
            "{} points to {}, expected {}",
            layout.command_executable.display(),
            target.display(),
            layout.service_executable.display()
        ))
    }
}

#[cfg(windows)]
fn command_registration(layout: &Layout) -> Result<String, String> {
    regular_file(&layout.command_executable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_counts_required_failures_and_warnings_independently() {
        let mut report = Report::new();
        report.required("required", Err("broken".into()));
        report.warning("warning", Err("uncertain".into()));
        report.info("info", "skipped");
        assert_eq!(report.failures, 1);
        assert_eq!(report.warnings, 1);
        assert_eq!(report.checks.len(), 3);
    }

    #[test]
    fn file_hash_requires_exact_content() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("config.json");
        fs::write(&path, b"content").expect("configuration");
        let hash = format!("{:x}", Sha256::digest(b"content"));
        assert!(file_hash(&path, &hash).is_ok());
        assert!(file_hash(&path, &"0".repeat(64)).is_err());
    }
}
