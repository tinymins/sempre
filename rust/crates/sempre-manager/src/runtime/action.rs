use chrono::Utc;
use sempre_state::{DesiredState, Document, RuntimeState};

use super::{
    RESTART, RuntimeStatus, START, STOP, action_error, ensure_runtime_deployment, is_transition,
    reset_runtime_intent,
};
use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn runtime_action(&self, action: &str) -> Result<RuntimeStatus, ManagerError> {
        self.runtime_action_inner(action, None).await
    }

    pub(crate) async fn runtime_action_inner(
        &self,
        action: &str,
        startup_task: Option<&str>,
    ) -> Result<RuntimeStatus, ManagerError> {
        if !matches!(action, START | STOP | RESTART) {
            return Err(action_error(
                "INVALID_RUNTIME_ACTION",
                "runtime action must be start, stop, or restart",
            ));
        }
        let mut cancellation = self.startup_tasks.cancellation();
        if startup_task.is_none() && self.startup_tasks.blocks(action) {
            return Err(crate::startup_task::busy());
        }
        if let Some(id) = startup_task {
            self.startup_tasks.with_preparation(id, || Ok(()))?;
        }
        let document = self.store.read()?;
        let current = self.runtime_status_value(&document);
        if action == STOP {
            if document.desired_state == DesiredState::Stopped && !self.startup_tasks.preparing() {
                return Ok(current);
            }
            // Stop intent must not wait for the preparation/runtime operation locks.
            self.startup_tasks.begin_stop();
            self.stage_runtime_action(STOP, &document)?;
            self.request_runtime_reload();
            return self.runtime_status();
        }
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
        if action == RESTART && is_transition(current.runtime_state) {
            return Err(crate::startup_task::busy());
        }
        let task_id = if let Some(id) = startup_task {
            id.to_owned()
        } else {
            self.startup_tasks
                .begin(
                    if action == START { START } else { RESTART },
                    current.pending_changes.clone(),
                )?
                .id
        };
        let operation = async {
            use crate::supervisor::lifecycle::{LOCK_TIMEOUT, PREPARE_TIMEOUT};
            let _runtime = self
                .lifecycle_phase(
                    "runtime_lock",
                    "Waiting for runtime configuration lock",
                    LOCK_TIMEOUT,
                    async { Ok::<_, ManagerError>(self.runtime_rules_gate.lock().await) },
                )
                .await?;
            let _operation = self.operation_lease().await?;
            let document = self.store.read()?;
            self.ensure_runtime_action_preparable(
                &document,
                self.active_profile_config_pending(&document),
            )?;
            self.lifecycle_phase(
                "compiling",
                "Compiling active subscription configuration",
                PREPARE_TIMEOUT,
                Box::pin(self.prepare_runtime_action()),
            )
            .await?;
            let document = self.store.read()?;
            let id = &task_id;
            let config = self.startup_tasks.with_preparation(id, || {
                self.stage_runtime_action(action, &document)?;
                self.current_config()
            })?;
            self.startup_tasks.prepared_for(id, config);
            self.request_runtime_reload();
            self.runtime_status()
        };
        let result = tokio::select! {
            biased;
            _ = cancellation.changed() => Err(action_error("RUNTIME_STARTUP_CANCELLED", "Core startup cancelled by stop request")),
            result = operation => result,
        };
        if let Err(error) = &result {
            self.startup_tasks.fail_for(&task_id, &error.to_string());
        }
        result
    }

    fn stage_runtime_action(&self, action: &str, document: &Document) -> Result<(), ManagerError> {
        let expected = self.runtime_deployment(document);
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
        Ok(())
    }

    async fn prepare_runtime_action(&self) -> Result<(), ManagerError> {
        if let Err(error) = self.prepare_active_subscription_for_runtime_locked().await {
            self.record_runtime_preparation_failure(&error)?;
            return Err(action_error(
                "RUNTIME_PREPARATION_FAILED",
                error.to_string(),
            ));
        }
        Ok(())
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
}
