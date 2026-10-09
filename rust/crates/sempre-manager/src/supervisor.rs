mod plan;
mod recovery;
mod state;
mod wait;

use std::{fs, path::PathBuf, process::ExitStatus, time::Duration};

use chrono::Utc;
use sempre_core::{CommandSpec, ControlSpec};
use sempre_state::{Deployment, DesiredState, Document};
use sempre_supervisor::{ManagedProcess, SupervisorError, append_log};
use sempre_transparent::Plan as TransparentPlan;
use tokio::sync::watch;

use crate::dns_runtime::DnsFrontendPlan;
use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};
use wait::{ProcessEvent, RetryEvent, wait_inactive, wait_retry, wait_running, wait_startup};

const STARTUP_GRACE: Duration = Duration::from_secs(10);
const STOP_GRACE: Duration = Duration::from_secs(10);
const RETRY_INTERVAL: Duration = Duration::from_secs(30);

pub(crate) struct RuntimePlan {
    pub(crate) deployment: Deployment,
    spec: CommandSpec,
    runtime_config: PathBuf,
    runtime_config_hash: String,
    control: Option<ControlSpec>,
    transparent: TransparentPlan,
    pub(crate) dns_frontend: Option<DnsFrontendPlan>,
    dns_rule_files: Option<crate::dns_rule_files::PreparedRuleFiles>,
    pub(crate) rules: crate::rule_bootstrap::RuleBootstrap,
}

enum CycleResult {
    Restart,
    Failed,
    Shutdown,
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn run_supervisor(
        &self,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), ManagerError> {
        let result = self
            .run_supervisor_with_grace(shutdown, STARTUP_GRACE)
            .await;
        self.restart_tasks.supervisor_exited(result.as_ref().err());
        result
    }

    async fn run_supervisor_with_grace(
        &self,
        mut shutdown: watch::Receiver<bool>,
        startup_grace: Duration,
    ) -> Result<(), ManagerError> {
        recovery::recover_stale_process(self).await?;
        self.transparent.recover_stale_system_dns().await?;
        loop {
            let document = self.store.read()?;
            if *shutdown.borrow() {
                self.dns_frontend.stop().await;
                self.stop_gateway().await;
                if transparent_cleanup_required(&document) {
                    self.transparent.cleanup().await?;
                }
                state::mark_intentional_exit(self, true)?;
                return Ok(());
            }
            if document.desired_state == DesiredState::Stopped {
                self.dns_frontend.stop().await;
                self.stop_gateway().await;
                if transparent_cleanup_required(&document) {
                    self.transparent.cleanup().await?;
                }
                state::mark_inactive(self, true)?;
                if wait_inactive(self, &mut shutdown).await {
                    return Ok(());
                }
                continue;
            }
            if document.active.is_none() {
                self.dns_frontend.stop().await;
                self.stop_gateway().await;
                if transparent_cleanup_required(&document) {
                    self.transparent.cleanup().await?;
                }
                state::mark_inactive(self, false)?;
                if wait_inactive(self, &mut shutdown).await {
                    return Ok(());
                }
                continue;
            }
            let runtime_gate = self.runtime_rules_gate.lock().await;
            let plan = match self.resolve_runtime_plan().await {
                Ok(plan) => plan,
                Err(error) => {
                    drop(runtime_gate);
                    let error =
                        with_cleanup_failure(&error, self.cleanup_after_core_failure().await);
                    self.log_supervisor(&format!("resolve deployment failed: {error}"));
                    state::record_failure(self, "resolve failed", &error, false)?;
                    match wait_retry(self, &mut shutdown, RETRY_INTERVAL).await {
                        RetryEvent::Timer => {}
                        RetryEvent::NetworkChanged => {
                            self.log_supervisor("network changed; retrying core immediately");
                        }
                        RetryEvent::Reload => self.cleanup_after_core_failure().await?,
                        RetryEvent::Shutdown => {
                            self.cleanup_retained_frontend().await?;
                            state::mark_intentional_exit(self, true)?;
                            return Ok(());
                        }
                    }
                    continue;
                }
            };
            if !state::mark_starting(self, &plan)? {
                continue;
            }
            match self
                .run_runtime_plan(&plan, &mut shutdown, startup_grace, runtime_gate)
                .await?
            {
                CycleResult::Restart => {}
                CycleResult::Shutdown => return Ok(()),
                CycleResult::Failed => {
                    match wait_retry(self, &mut shutdown, RETRY_INTERVAL).await {
                        RetryEvent::Timer => {}
                        RetryEvent::NetworkChanged => {
                            self.log_supervisor("network changed; retrying core immediately");
                        }
                        RetryEvent::Reload => self.cleanup_retained_frontend().await?,
                        RetryEvent::Shutdown => {
                            self.cleanup_retained_frontend().await?;
                            state::mark_intentional_exit(self, true)?;
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    async fn run_runtime_plan(
        &self,
        plan: &RuntimePlan,
        shutdown: &mut watch::Receiver<bool>,
        startup_grace: Duration,
        runtime_gate: tokio::sync::MutexGuard<'_, ()>,
    ) -> Result<CycleResult, ManagerError> {
        self.log_supervisor(&format!("starting {}", deployment_label(&plan.deployment)));
        self.restart_tasks
            .runtime_log("starting", &deployment_label(&plan.deployment));
        if let Err(error) = self.start_gateway().await {
            self.handle_process_failure(plan, "gateway startup failed", &error)?;
            return Ok(CycleResult::Failed);
        }
        let tasks = self.restart_tasks.clone();
        let mut process = match ManagedProcess::spawn_observed(
            &plan.spec,
            &self.store.layout().core_stdout_log,
            &self.store.layout().core_stderr_log,
            Some(std::sync::Arc::new(move |stream, line| {
                tasks.runtime_log(stream, line);
            })),
        ) {
            Ok(process) => process,
            Err(error) => {
                let cleanup = self.cleanup_after_core_failure().await;
                let error = with_cleanup_failure(&error, cleanup);
                self.handle_process_failure(plan, "startup failed", &error)?;
                return Ok(CycleResult::Failed);
            }
        };
        let setup = self.mark_runtime_started(plan, process.pid());
        if let Err(error) = setup {
            let _ = process.terminate(STOP_GRACE).await;
            if let Err(cleanup) = self.cleanup_after_core_failure().await {
                self.log_supervisor(&format!("transparent proxy cleanup failed: {cleanup}"));
            }
            self.remove_control();
            return Err(error);
        }

        match wait_startup(self, shutdown, &mut process, plan, startup_grace).await {
            ProcessEvent::Healthy(Ok(())) => {
                process.synchronize_output().await;
                if let Err(error) = self.mark_runtime_healthy(plan) {
                    let _ = process.terminate(STOP_GRACE).await;
                    if let Err(cleanup) = self.cleanup_after_core_failure().await {
                        self.log_supervisor(&format!(
                            "transparent proxy cleanup failed: {cleanup}"
                        ));
                    }
                    self.remove_control();
                    return Err(error);
                }
            }
            ProcessEvent::Healthy(Err(error)) => {
                return self
                    .fail_transparent_startup(plan, &mut process, &error)
                    .await;
            }
            ProcessEvent::Reload => {
                drop(runtime_gate);
                self.stop_process(&mut process, false).await?;
                return Ok(CycleResult::Restart);
            }
            ProcessEvent::Shutdown => {
                drop(runtime_gate);
                self.stop_process(&mut process, true).await?;
                return Ok(CycleResult::Shutdown);
            }
            ProcessEvent::Exited(result) => {
                let cleanup = self.cleanup_after_core_failure().await;
                self.remove_control();
                let exit = exit_result(result);
                let error = with_cleanup_failure(&exit, cleanup);
                self.handle_process_failure(plan, "startup failed", &error)?;
                return Ok(CycleResult::Failed);
            }
        }

        drop(runtime_gate);
        match wait_running(self, shutdown, &mut process, plan).await {
            ProcessEvent::Reload => {
                self.stop_process(&mut process, false).await?;
                Ok(CycleResult::Restart)
            }
            ProcessEvent::Shutdown => {
                self.stop_process(&mut process, true).await?;
                Ok(CycleResult::Shutdown)
            }
            ProcessEvent::Exited(result) => {
                let _runtime_gate = self.runtime_rules_gate.lock().await;
                self.remove_control();
                let exit = exit_result(result);
                let error = with_cleanup_failure(&exit, self.cleanup_after_core_failure().await);
                self.dns_frontend.record_failure(&error);
                self.handle_process_failure(plan, "core exited", &error)?;
                Ok(CycleResult::Failed)
            }
            ProcessEvent::Healthy(_) => unreachable!("running process has no startup timer"),
        }
    }

    async fn stop_process(
        &self,
        process: &mut ManagedProcess,
        service_stopped: bool,
    ) -> Result<(), ManagerError> {
        let _runtime_gate = self.runtime_rules_gate.lock().await;
        let transition = state::mark_stopping(self);
        self.log_restart_stopping(process.pid());
        let transparent = if service_stopped {
            self.dns_frontend.stop().await;
            self.stop_gateway().await;
            self.transparent.cleanup().await
        } else {
            self.cleanup_after_core_failure().await
        };
        let terminated = process.terminate(STOP_GRACE).await;
        self.restart_tasks.stopped(&terminated);
        self.remove_control();
        transition?;
        let state = state::mark_intentional_exit(self, service_stopped);
        transparent?;
        terminated?;
        state
    }

    async fn cleanup_retained_frontend(&self) -> Result<(), ManagerError> {
        self.dns_frontend.stop().await;
        self.stop_gateway().await;
        self.transparent.cleanup().await?;
        Ok(())
    }

    fn mark_runtime_healthy(&self, plan: &RuntimePlan) -> Result<(), ManagerError> {
        state::mark_healthy(self, plan)?;
        self.log_supervisor(&format!(
            "healthy {}; pending online rule sets: {}",
            deployment_label(&plan.deployment),
            plan.rules.pending_count()
        ));
        self.restart_tasks.healthy();
        Ok(())
    }

    fn mark_runtime_started(&self, plan: &RuntimePlan, pid: u32) -> Result<(), ManagerError> {
        self.restart_tasks.runtime_log(
            "health_check",
            &format!("{} · PID {pid}", deployment_label(&plan.deployment)),
        );
        state::mark_started(self, plan, pid)
            .and_then(|()| self.write_control(plan.control.as_ref()))?;
        if let Some(prepared) = &plan.dns_rule_files {
            crate::dns_rule_files::record_started(prepared, &self.store.read()?)?;
        }
        self.log_supervisor(&format!(
            "started {} with PID {pid}",
            deployment_label(&plan.deployment)
        ));
        Ok(())
    }

    async fn fail_transparent_startup(
        &self,
        plan: &RuntimePlan,
        process: &mut ManagedProcess,
        error: &sempre_transparent::TransparentError,
    ) -> Result<CycleResult, ManagerError> {
        let _ = process.terminate(STOP_GRACE).await;
        let error = with_cleanup_failure(error, self.cleanup_after_core_failure().await);
        self.remove_control();
        self.handle_process_failure(plan, "transparent proxy startup failed", &error)?;
        Ok(CycleResult::Failed)
    }

    fn handle_process_failure(
        &self,
        plan: &RuntimePlan,
        stage: &str,
        error: &impl ToString,
    ) -> Result<(), ManagerError> {
        let message = error.to_string();
        self.log_supervisor(&format!(
            "{stage} for {}: {message}",
            deployment_label(&plan.deployment)
        ));
        state::record_failure(self, stage, &message, true)
    }

    fn write_control(&self, control: Option<&ControlSpec>) -> Result<(), ManagerError> {
        let Some(control) = control else {
            self.remove_control();
            return Ok(());
        };
        let mut data = serde_json::to_vec_pretty(&serde_json::json!({
            "core": control.core,
            "protocol": control.protocol,
            "base_url": control.base_url,
            "secret": control.secret,
        }))
        .map_err(|error| ManagerError::RuntimeNotReady(error.to_string()))?;
        data.push(b'\n');
        sempre_state::write_atomic(&self.store.layout().core_control, &data, 0o600)
            .map_err(|error| ManagerError::io("write core control endpoint", error))
    }

    fn remove_control(&self) {
        let _ = fs::remove_file(&self.store.layout().core_control);
    }

    pub(crate) fn log_supervisor(&self, message: &str) {
        self.restart_tasks.runtime_log("supervisor", message);
        let line = format!("{} {message}\n", Utc::now().to_rfc3339());
        // Disk-backed diagnostics must not affect the managed runtime lifecycle.
        let _ = append_log(&self.store.layout().manager_log, &line);
    }
}

fn exit_result(result: Result<ExitStatus, SupervisorError>) -> String {
    match result {
        Ok(status) if status.success() => "exited successfully".into(),
        Ok(status) => status.to_string(),
        Err(error) => error.to_string(),
    }
}

fn with_cleanup_failure(
    error: &impl ToString,
    cleanup: Result<(), sempre_transparent::TransparentError>,
) -> String {
    let error = error.to_string();
    match cleanup {
        Ok(()) => error,
        Err(cleanup) => format!("{error}; transparent proxy cleanup failed: {cleanup}"),
    }
}

fn deployment_label(deployment: &Deployment) -> String {
    format!("{}@{}", deployment.core, deployment.version)
}

fn transparent_cleanup_required(document: &Document) -> bool {
    document.active.is_some() || document.runtime.core.is_some()
}

fn path_text(path: &std::path::Path) -> Result<&str, ManagerError> {
    path.to_str()
        .ok_or_else(|| ManagerError::NonUnicodePath(path.into()))
}

#[cfg(test)]
mod tests;
