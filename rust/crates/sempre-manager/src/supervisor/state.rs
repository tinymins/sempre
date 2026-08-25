use chrono::Utc;
use sempre_state::{DesiredState, Document, RuntimeFailure, RuntimeState};

use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

use super::RuntimePlan;

pub fn mark_inactive<R: VersionRunner>(
    manager: &Manager<R>,
    stopped: bool,
) -> Result<(), ManagerError> {
    manager.store.update(|document| {
        document.runtime.pid = None;
        if stopped || document.desired_state == DesiredState::Stopped {
            document.runtime.state = RuntimeState::Stopped;
            document.runtime.last_exit = Some("stopped by user".into());
        } else if document.runtime.last_error.is_some() || document.last_error.is_some() {
            document.runtime.state = RuntimeState::Failed;
        } else {
            document.runtime = sempre_state::Runtime {
                state: RuntimeState::Idle,
                last_transition: Some(Utc::now()),
                ..sempre_state::Runtime::default()
            };
        }
        Ok(())
    })?;
    Ok(())
}

pub fn mark_starting<R: VersionRunner>(
    manager: &Manager<R>,
    plan: &RuntimePlan,
) -> Result<bool, ManagerError> {
    let mut allowed = false;
    manager.store.update(|document| {
        allowed = document.desired_state == DesiredState::Running
            && document.active.as_ref() == Some(&plan.deployment);
        if allowed {
            fill_runtime(document, plan);
            document.runtime.state = RuntimeState::Starting;
            document.runtime.pid = None;
            document.runtime.started_at = None;
            document.runtime.last_transition = Some(Utc::now());
        }
        Ok(())
    })?;
    Ok(allowed)
}

pub fn mark_started<R: VersionRunner>(
    manager: &Manager<R>,
    plan: &RuntimePlan,
    pid: u32,
) -> Result<(), ManagerError> {
    manager.store.update(|document| {
        fill_runtime(document, plan);
        document.runtime.state = RuntimeState::Starting;
        document.runtime.pid = Some(pid);
        document.runtime.started_at = Some(Utc::now());
        document.runtime.last_transition = Some(Utc::now());
        Ok(())
    })?;
    Ok(())
}

pub fn mark_healthy<R: VersionRunner>(
    manager: &Manager<R>,
    plan: &RuntimePlan,
) -> Result<(), ManagerError> {
    manager.store.update(|document| {
        if document.active.as_ref() != Some(&plan.deployment) {
            return Ok(());
        }
        if document.pending {
            document.previous = None;
            document.pending = false;
        }
        document.last_error = None;
        document.runtime.state = RuntimeState::Running;
        document.runtime.last_error = None;
        document.runtime.last_failure = None;
        document.runtime.last_transition = Some(Utc::now());
        Ok(())
    })?;
    Ok(())
}

pub fn mark_stopping<R: VersionRunner>(manager: &Manager<R>) -> Result<(), ManagerError> {
    manager.store.update(|document| {
        document.runtime.state = RuntimeState::Stopping;
        document.runtime.last_exit = Some(
            if document.desired_state == DesiredState::Stopped {
                "stopped by user"
            } else {
                "restart requested"
            }
            .into(),
        );
        document.runtime.last_transition = Some(Utc::now());
        Ok(())
    })?;
    Ok(())
}

pub fn mark_intentional_exit<R: VersionRunner>(
    manager: &Manager<R>,
    service_stopped: bool,
) -> Result<(), ManagerError> {
    manager.store.update(|document| {
        document.runtime.pid = None;
        document.runtime.state =
            if service_stopped || document.desired_state == DesiredState::Stopped {
                RuntimeState::Stopped
            } else {
                RuntimeState::Restarting
            };
        document.runtime.last_exit = Some(if service_stopped {
            "Sempre service stopped".into()
        } else if document.desired_state == DesiredState::Stopped {
            "stopped by user".into()
        } else {
            "restart requested".into()
        });
        document.runtime.last_transition = Some(Utc::now());
        Ok(())
    })?;
    Ok(())
}

pub fn record_failure<R: VersionRunner + ValidationRunner>(
    manager: &Manager<R>,
    stage: &str,
    error: &str,
    rollback_pending: bool,
    increment_restart: bool,
) -> Result<bool, ManagerError> {
    let mut retry = false;
    manager.store.update(|document| {
        let now = Utc::now();
        let failed = document.active.clone();
        let mut failure = RuntimeFailure {
            stage: stage.into(),
            error: error.into(),
            occurred_at: now,
            failed,
            rolled_back_to: None,
        };
        document.last_error = Some(format!("{stage}: {error}"));
        if rollback_pending && document.pending {
            if let Some(restored) = document.previous.take() {
                failure.rolled_back_to = Some(restored.clone());
                if failure
                    .failed
                    .as_ref()
                    .is_some_and(|item| item.core == restored.core)
                {
                    document.config_builds.remove(&restored.core);
                }
                document
                    .configs
                    .insert(restored.core.clone(), restored.config_hash.clone());
                document.active = Some(restored);
                retry = true;
            } else {
                document.active = None;
            }
            document.pending = false;
        }
        document.runtime.state = if document.desired_state == DesiredState::Stopped {
            RuntimeState::Stopped
        } else {
            RuntimeState::Failed
        };
        document.runtime.pid = None;
        if increment_restart {
            document.runtime.restart_count = document.runtime.restart_count.saturating_add(1);
        }
        document.runtime.last_exit = Some(error.into());
        document.runtime.last_error = Some(error.into());
        document.runtime.last_failure = Some(failure);
        document.runtime.last_transition = Some(now);
        Ok(())
    })?;
    Ok(retry)
}

fn fill_runtime(document: &mut Document, plan: &RuntimePlan) {
    document.runtime.core = Some(plan.deployment.core.clone());
    document
        .runtime
        .repository
        .clone_from(&plan.deployment.repository);
    document.runtime.reference = Some(plan.deployment.reference.clone());
    document.runtime.version = Some(plan.deployment.version.clone());
    document.runtime.config_hash = Some(plan.deployment.config_hash.clone());
    document.runtime.runtime_config = Some(plan.runtime_config.display().to_string());
    document.runtime.runtime_config_hash = Some(plan.runtime_config_hash.clone());
}
