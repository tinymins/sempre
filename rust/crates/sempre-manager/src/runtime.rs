use chrono::{DateTime, Utc};
use sempre_core::CoreRef;
use sempre_state::{Deployment, DesiredState, Document, RuntimeFailure, RuntimeState};
use serde::{Deserialize, Serialize};
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::{
    Manager, ManagerError, RuntimePendingChange, ValidationRunner, VersionRunner,
    config::configuration_target,
};

const START: &str = "start";
const STOP: &str = "stop";
const RESTART: &str = "restart";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuntimeActionAvailability {
    pub allowed: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuntimeActions {
    pub start: RuntimeActionAvailability,
    pub stop: RuntimeActionAvailability,
    pub restart: RuntimeActionAvailability,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuntimeDeployment {
    pub core: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(rename = "ref")]
    pub reference: String,
    pub version: String,
    pub exact_reference: String,
    pub config_hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuntimeFailureOutput {
    pub stage: String,
    pub error: String,
    pub occurred_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<RuntimeDeployment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rolled_back_to: Option<RuntimeDeployment>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuntimeStatus {
    pub desired_state: DesiredState,
    pub runtime_state: RuntimeState,
    pub active: Option<RuntimeDeployment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<RuntimeDeployment>,
    pub pid: u32,
    pub started_at: Option<DateTime<Utc>>,
    pub uptime_seconds: i64,
    pub restart_count: u32,
    pub pending: bool,
    pub pending_changes: Vec<RuntimePendingChange>,
    pub last_transition: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_exit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<RuntimeFailureOutput>,
    pub actions: RuntimeActions,
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub fn runtime_status(&self) -> Result<RuntimeStatus, ManagerError> {
        Ok(self.runtime_status_value(&self.store.read()?))
    }

    pub async fn runtime_action(&self, action: &str) -> Result<RuntimeStatus, ManagerError> {
        if !matches!(action, START | STOP | RESTART) {
            return Err(action_error(
                "INVALID_RUNTIME_ACTION",
                "runtime action must be start, stop, or restart",
            ));
        }
        let _operation = self.store.acquire_operation()?;
        let mut document = self.store.read()?;
        let current = self.runtime_status_value(&document);
        let configuration_pending = self.active_profile_config_pending(&document);
        if action == START
            && document.desired_state == DesiredState::Running
            && matches!(
                current.runtime_state,
                RuntimeState::Running | RuntimeState::Starting | RuntimeState::Restarting
            )
            && !configuration_pending
        {
            return Ok(current);
        }
        if action == RESTART
            && document.desired_state == DesiredState::Running
            && is_transition(current.runtime_state)
            && !configuration_pending
        {
            return Ok(current);
        }
        if matches!(action, START | RESTART) {
            self.ensure_runtime_action_preparable(&document, configuration_pending)?;
            if let Err(error) = self.prepare_active_subscription_for_runtime_locked().await {
                self.record_runtime_preparation_failure(&error)?;
                return Err(action_error(
                    "RUNTIME_PREPARATION_FAILED",
                    error.to_string(),
                ));
            }
            document = self.store.read()?;
        }
        let expected = self.runtime_deployment(&document);
        match action {
            START => {
                let expected = expected
                    .map_err(|error| action_error("RUNTIME_NOT_READY", error.to_string()))?;
                self.store
                    .update_checked(|state| -> Result<(), ManagerError> {
                        ensure_runtime_deployment(self, state, &expected)?;
                        state.desired_state = DesiredState::Running;
                        state.runtime.state = if state.runtime.state == RuntimeState::Stopping {
                            RuntimeState::Restarting
                        } else {
                            RuntimeState::Starting
                        };
                        reset_runtime_intent(state);
                        Ok(())
                    })?;
            }
            STOP => {
                if document.desired_state == DesiredState::Stopped {
                    return Ok(current);
                }
                self.store.update(|state| {
                    state.desired_state = DesiredState::Stopped;
                    state.runtime.state = if state.runtime.pid.is_some()
                        || is_transition(state.runtime.state)
                        || state.runtime.state == RuntimeState::Running
                    {
                        RuntimeState::Stopping
                    } else {
                        state.runtime.pid = None;
                        RuntimeState::Stopped
                    };
                    state.runtime.last_transition = Some(Utc::now());
                    Ok(())
                })?;
            }
            RESTART => {
                let expected = expected
                    .map_err(|error| action_error("RUNTIME_NOT_READY", error.to_string()))?;
                self.store
                    .update_checked(|state| -> Result<(), ManagerError> {
                        ensure_runtime_deployment(self, state, &expected)?;
                        state.desired_state = DesiredState::Running;
                        state.runtime.state = if state.runtime.pid.is_some()
                            || state.runtime.state == RuntimeState::Running
                        {
                            RuntimeState::Stopping
                        } else {
                            state.runtime.pid = None;
                            RuntimeState::Restarting
                        };
                        reset_runtime_intent(state);
                        Ok(())
                    })?;
            }
            _ => unreachable!("validated runtime action"),
        }
        self.request_runtime_reload();
        self.runtime_status()
    }

    fn ensure_runtime_action_preparable(
        &self,
        document: &Document,
        configuration_pending: bool,
    ) -> Result<(), ManagerError> {
        match self.runtime_deployment(document) {
            Err(ManagerError::NoConfiguration) if configuration_pending => Ok(()),
            Err(error) => Err(action_error("RUNTIME_NOT_READY", error.to_string())),
            Ok(_) => Ok(()),
        }
    }

    fn runtime_status_value(&self, document: &Document) -> RuntimeStatus {
        let target = self.runtime_deployment(document);
        let configuration_pending = self.active_profile_config_pending(document);
        let mut runtime_state = document.runtime.state;
        let mut last_error = document.runtime.last_error.clone();
        if runtime_state == RuntimeState::Idle && document.desired_state == DesiredState::Stopped {
            runtime_state = RuntimeState::Stopped;
        } else if runtime_state == RuntimeState::Idle
            && last_error
                .as_ref()
                .or(document.last_error.as_ref())
                .is_some()
            && target.is_ok()
        {
            runtime_state = RuntimeState::Failed;
        }
        if last_error.is_none() && runtime_state != RuntimeState::Running {
            last_error.clone_from(&document.last_error);
        }
        if let Some(pid) = document.runtime.pid
            && runtime_state != RuntimeState::Stopping
            && !process_alive(pid)
        {
            runtime_state = RuntimeState::Failed;
            last_error = Some(format!("recorded PID {pid} is not running"));
        }
        let alive = document.runtime.pid.is_some_and(process_alive);
        let uptime_seconds = document.runtime.started_at.map_or(0, |started| {
            if alive {
                (Utc::now() - started).num_seconds().max(0)
            } else {
                0
            }
        });
        RuntimeStatus {
            desired_state: document.desired_state,
            runtime_state,
            active: document.active.clone().map(deployment_value),
            target: if document.active.is_none() {
                target.as_ref().ok().cloned().map(deployment_value)
            } else {
                None
            },
            pid: document.runtime.pid.unwrap_or(0),
            started_at: document.runtime.started_at,
            uptime_seconds,
            restart_count: document.runtime.restart_count,
            pending: document.pending || configuration_pending,
            pending_changes: self.runtime_pending_changes(document, configuration_pending),
            last_transition: document.runtime.last_transition,
            last_exit: document.runtime.last_exit.clone(),
            last_error,
            last_failure: document.runtime.last_failure.clone().map(failure_value),
            actions: runtime_actions(
                document,
                runtime_state,
                target.as_ref().err(),
                configuration_pending,
            ),
        }
    }

    fn runtime_deployment(&self, document: &Document) -> Result<Deployment, ManagerError> {
        let deployment = if let Some(active) = &document.active {
            active.clone()
        } else {
            let (reference, version) = configuration_target(document)?;
            let config_hash = document
                .configs
                .get(&reference.core)
                .filter(|hash| !hash.is_empty())
                .cloned()
                .ok_or(ManagerError::NoConfiguration)?;
            Deployment {
                core: reference.core,
                repository: reference.repository,
                reference: reference.reference,
                version,
                config_hash,
            }
        };
        let binary = self.store.layout().core_binary(
            &deployment.core,
            deployment.repository.as_deref(),
            &deployment.version,
        );
        if !binary.is_file() {
            return Err(ManagerError::RuntimeNotReady(
                "managed core binary is unavailable".into(),
            ));
        }
        let config = self
            .store
            .layout()
            .config(&deployment.core, &deployment.config_hash);
        if !config.is_file() {
            return Err(ManagerError::RuntimeNotReady(
                "managed configuration is unavailable".into(),
            ));
        }
        Ok(deployment)
    }

    fn record_runtime_preparation_failure(&self, error: &ManagerError) -> Result<(), ManagerError> {
        self.store.update(|document| {
            let now = Utc::now();
            document.last_error = Some(format!("prepare runtime configuration: {error}"));
            document.runtime.last_error = Some(error.to_string());
            document.runtime.last_failure = Some(RuntimeFailure {
                stage: "prepare runtime configuration".into(),
                error: error.to_string(),
                occurred_at: now,
                failed: None,
                rolled_back_to: document.active.clone(),
            });
            document.runtime.last_transition = Some(now);
            Ok(())
        })?;
        Ok(())
    }
}

fn ensure_runtime_deployment<R: VersionRunner + ValidationRunner>(
    manager: &Manager<R>,
    document: &mut Document,
    expected: &Deployment,
) -> Result<(), ManagerError> {
    let current = manager.runtime_deployment(document)?;
    if &current != expected {
        return Err(ManagerError::RuntimeNotReady(
            "managed core deployment changed while applying the runtime action; retry".into(),
        ));
    }
    if document.active.is_none() {
        document.stage(current);
    }
    Ok(())
}

fn reset_runtime_intent(document: &mut Document) {
    document.runtime.pid = None;
    document.runtime.last_error = None;
    document.runtime.last_failure = None;
    document.runtime.last_transition = Some(Utc::now());
}

fn runtime_actions(
    document: &Document,
    state: RuntimeState,
    readiness: Option<&ManagerError>,
    configuration_pending: bool,
) -> RuntimeActions {
    let configuration_can_be_prepared =
        configuration_pending && matches!(readiness, Some(ManagerError::NoConfiguration));
    let allowed = readiness.is_none() || configuration_can_be_prepared;
    let reason = if allowed {
        String::new()
    } else {
        readiness.map_or_else(String::new, ToString::to_string)
    };
    let mut start = RuntimeActionAvailability {
        allowed,
        reason: reason.clone(),
    };
    let mut restart = RuntimeActionAvailability { allowed, reason };
    let mut stop = RuntimeActionAvailability {
        allowed: document.desired_state == DesiredState::Running,
        reason: String::new(),
    };
    match state {
        RuntimeState::Running => {
            start.allowed = false;
            start.reason = "managed core is already running".into();
        }
        RuntimeState::Starting | RuntimeState::Restarting | RuntimeState::Stopping => {
            let message = format!("managed core is {}", state_label(state));
            start.allowed = false;
            start.reason.clone_from(&message);
            restart.allowed = false;
            restart.reason = message;
        }
        _ => {}
    }
    if document.desired_state == DesiredState::Stopped {
        stop.allowed = false;
        stop.reason = "managed core is already stopped".into();
    }
    RuntimeActions {
        start,
        stop,
        restart,
    }
}

fn deployment_value(deployment: Deployment) -> RuntimeDeployment {
    let exact_reference = CoreRef {
        core: deployment.core.clone(),
        repository: deployment.repository.clone(),
        reference: deployment.version.clone(),
    }
    .to_string();
    RuntimeDeployment {
        core: deployment.core,
        repository: deployment.repository,
        reference: deployment.reference,
        version: deployment.version,
        exact_reference,
        config_hash: deployment.config_hash,
    }
}

fn failure_value(failure: RuntimeFailure) -> RuntimeFailureOutput {
    RuntimeFailureOutput {
        stage: failure.stage,
        error: failure.error,
        occurred_at: failure.occurred_at,
        failed: failure.failed.map(deployment_value),
        rolled_back_to: failure.rolled_back_to.map(deployment_value),
    }
}

fn process_alive(pid: u32) -> bool {
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).is_some()
}

const fn is_transition(state: RuntimeState) -> bool {
    matches!(
        state,
        RuntimeState::Starting | RuntimeState::Stopping | RuntimeState::Restarting
    )
}

const fn state_label(state: RuntimeState) -> &'static str {
    match state {
        RuntimeState::Starting => "starting",
        RuntimeState::Stopping => "stopping",
        RuntimeState::Restarting => "restarting",
        _ => "transitioning",
    }
}

fn action_error(code: &'static str, message: impl Into<String>) -> ManagerError {
    ManagerError::RuntimeAction {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests;
