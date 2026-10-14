use chrono::Utc;
use sempre_state::{DesiredState, Document, RuntimeState};

use super::{
    RESTART, RuntimeStatus, START, STOP, action_error, ensure_runtime_deployment, is_transition,
    reset_runtime_intent,
};
use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn runtime_action(&self, action: &str) -> Result<RuntimeStatus, ManagerError> {
        self.runtime_action_inner(action, false).await
    }

    pub(crate) async fn runtime_action_inner(
        &self,
        action: &str,
        startup_task: bool,
    ) -> Result<RuntimeStatus, ManagerError> {
        if !matches!(action, START | STOP | RESTART) {
            return Err(action_error(
                "INVALID_RUNTIME_ACTION",
                "runtime action must be start, stop, or restart",
            ));
        }
        if !startup_task && self.startup_tasks.blocks(action) {
            return Err(crate::startup_task::busy());
        }
        let _runtime = if action == STOP {
            None
        } else {
            Some(self.runtime_rules_gate.lock().await)
        };
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
        if action == RESTART && is_transition(current.runtime_state) {
            return Err(crate::startup_task::busy());
        }
        if action == STOP && document.desired_state == DesiredState::Stopped {
            return Ok(current);
        }
        let recording = matches!(action, START | RESTART);
        if recording && !startup_task {
            self.startup_tasks.begin(
                if action == START { START } else { RESTART },
                current.pending_changes.clone(),
            )?;
        }
        let result = async {
            if recording {
                self.ensure_runtime_action_preparable(&document, configuration_pending)?;
                self.startup_tasks.log("compiling", "");
                self.prepare_runtime_action().await?;
                document = self.store.read()?;
            }
            self.stage_runtime_action(action, &document)?;
            if recording {
                self.startup_tasks.prepared(self.current_config()?);
            } else {
                self.startup_tasks.fail("Core startup stopped");
            }
            self.request_runtime_reload();
            self.runtime_status()
        }
        .await;
        if recording && let Err(error) = &result {
            self.startup_tasks.fail(&error.to_string());
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
