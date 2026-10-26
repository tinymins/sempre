pub(crate) mod lifecycle;
mod plan;
mod recovery;
mod state;
mod wait;

use std::{fs, path::PathBuf, process::ExitStatus, time::Duration};

use chrono::Utc;
use sempre_core::{CommandSpec, ControlSpec};
use sempre_state::{Deployment, DesiredState};
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
    pub(crate) transparent: TransparentPlan,
    pub(crate) cn_source: Option<crate::cn_routes::Source>,
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
        self.startup_tasks.supervisor_exited(result.as_ref().err());
        result
    }

    async fn run_supervisor_with_grace(
        &self,
        mut shutdown: watch::Receiver<bool>,
        startup_grace: Duration,
    ) -> Result<(), ManagerError> {
        let initial = self.recover_lifecycle().await?;
        let mut cleanup_needed = transparent_cleanup_required(&initial);
        loop {
            let document = self.store.read()?;
            if *shutdown.borrow() {
                let cleanup = if cleanup_needed {
                    self.cleanup_retained_frontend().await
                } else {
                    Ok(())
                };
                state::mark_intentional_exit(self, true)?;
                self.startup_tasks
                    .complete_stop(cleanup.as_ref().err().map(ToString::to_string).as_deref());
                return cleanup;
            }
            let service_stopped = document.desired_state == DesiredState::Stopped;
            if service_stopped || document.active.is_none() {
                let cleanup = self.cleanup_inactive(cleanup_needed).await;
                cleanup_needed = false;
                state::mark_inactive(self, service_stopped)?;
                self.startup_tasks
                    .complete_stop(cleanup.as_ref().err().map(ToString::to_string).as_deref());
                if let Err(error) = cleanup {
                    self.log_supervisor(&format!("inactive cleanup failed: {error}"));
                }
                if wait_inactive(self, &mut shutdown).await {
                    return Ok(());
                }
                continue;
            }
            if !self.startup_tasks.begin_cycle() {
                if wait::wait_preparation(self, &mut shutdown).await {
                    return Ok(());
                }
                continue;
            }
            cleanup_needed = true;
            let prepared = self.prepare_cycle(&mut shutdown).await;
            let (runtime_gate, plan) = match prepared {
                Ok(Some(prepared)) => prepared,
                Ok(None) => continue,
                Err(error) => {
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
                self.startup_tasks.interrupt_cycle();
                continue;
            }
            match self
                .run_runtime_plan(&plan, &mut shutdown, startup_grace, runtime_gate)
                .await?
            {
                CycleResult::Restart => {
                    cleanup_needed = false;
                }
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
        self.startup_tasks
            .runtime_log("starting", &deployment_label(&plan.deployment));
        if let Some(result) = self.start_runtime_gateway(plan, shutdown).await? {
            return Ok(result);
        }
        let mut process = match ManagedProcess::spawn_observed(
            &plan.spec,
            &self.store.layout().core_stdout_log,
            &self.store.layout().core_stderr_log,
            Some(self.startup_tasks.output_observer()),
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
            process.terminate(STOP_GRACE).await?;
            if let Err(cleanup) = self.cleanup_after_core_failure().await {
                self.log_supervisor(&format!("transparent proxy cleanup failed: {cleanup}"));
            }
            self.remove_control();
            return Err(error);
        }

        match wait_startup(self, shutdown, &mut process, plan, startup_grace).await {
            ProcessEvent::StartupFailed(error) => {
                self.fail_network_startup(plan, &mut process, &error)
                    .await?;
                return Ok(CycleResult::Failed);
            }
            ProcessEvent::Healthy => {
                process.synchronize_output().await;
                if let Err(error) = self.mark_runtime_healthy(plan) {
                    process.terminate(STOP_GRACE).await?;
                    if let Err(cleanup) = self.cleanup_after_core_failure().await {
                        self.log_supervisor(&format!(
                            "transparent proxy cleanup failed: {cleanup}"
                        ));
                    }
                    self.remove_control();
                    return Err(error);
                }
            }
            ProcessEvent::Reload => {
                self.startup_tasks.interrupt_cycle();
                drop(runtime_gate);
                self.stop_process(&mut process, false).await?;
                return Ok(CycleResult::Restart);
            }
            ProcessEvent::Shutdown => {
                self.startup_tasks.interrupt_cycle();
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
                self.remove_control();
                let exit = exit_result(result);
                let error = with_cleanup_failure(&exit, self.cleanup_after_core_failure().await);
                self.dns_frontend.record_failure(&error);
                self.handle_process_failure(plan, "core exited", &error)?;
                Ok(CycleResult::Failed)
            }
            ProcessEvent::Healthy | ProcessEvent::StartupFailed(_) => {
                unreachable!("running process has no startup timer")
            }
        }
    }

    async fn stop_process(
        &self,
        process: &mut ManagedProcess,
        service_stopped: bool,
    ) -> Result<(), ManagerError> {
        let transition = state::mark_stopping(self);
        self.log_startup_stopping(process.pid());
        self.startup_tasks.log(
            "stopping",
            "Waiting for core termination: 10s graceful, 5s forced, 2s output drain",
        );
        let terminated = process.terminate(STOP_GRACE).await;
        self.startup_tasks.stopped(&terminated);
        if let Err(error) = &terminated {
            self.startup_tasks.complete_stop(Some(&error.to_string()));
            self.log_supervisor(&format!("core termination failed: {error}"));
        }
        terminated?;
        self.remove_control();
        transition?;
        state::mark_intentional_exit(self, service_stopped)?;
        let stopped = service_stopped || self.store.read()?.desired_state == DesiredState::Stopped;
        let cleanup = self.cleanup_lifecycle(!stopped).await;
        self.startup_tasks
            .complete_stop(cleanup.as_ref().err().map(ToString::to_string).as_deref());
        if let Err(error) = cleanup {
            self.log_supervisor(&format!("core stopped; cleanup failed: {error}"));
        }
        Ok(())
    }

    async fn cleanup_retained_frontend(&self) -> Result<(), ManagerError> {
        self.cleanup_lifecycle(false).await
    }

    async fn cleanup_inactive(&self, runtime: bool) -> Result<(), ManagerError> {
        let cleanup = if runtime {
            self.cleanup_retained_frontend().await
        } else {
            Ok(())
        };
        // CN routes outlive core restarts; only an inactive runtime removes them.
        let routes = self
            .route_phase(
                "Removing managed CN routes",
                lifecycle::ROUTE_STALL_TIMEOUT,
                self.transparent.cleanup_cn_routes(),
            )
            .await;
        cleanup.and(routes)
    }

    fn mark_runtime_healthy(&self, plan: &RuntimePlan) -> Result<(), ManagerError> {
        state::mark_healthy(self, plan)?;
        self.log_supervisor(&format!(
            "healthy {}; pending online rule sets: {}",
            deployment_label(&plan.deployment),
            plan.rules.pending_count()
        ));
        self.startup_tasks.healthy();
        Ok(())
    }

    fn mark_runtime_started(&self, plan: &RuntimePlan, pid: u32) -> Result<(), ManagerError> {
        self.startup_tasks.runtime_log(
            "health_check",
            &format!("{} · PID {pid}", deployment_label(&plan.deployment)),
        );
        state::mark_started(self, plan, pid)
            .and_then(|()| self.write_control(plan.control.as_ref()))?;
        if let Some(prepared) = &plan.dns_rule_files
            && let Err(error) = crate::dns_rule_files::record_started(prepared, &self.store.read()?)
        {
            self.log_supervisor(&format!("DNS rule runtime snapshot failed: {error}"));
        }
        self.log_supervisor(&format!(
            "started {} with PID {pid}",
            deployment_label(&plan.deployment)
        ));
        Ok(())
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
        self.startup_tasks.runtime_log("supervisor", message);
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

fn with_cleanup_failure(error: &impl ToString, cleanup: Result<(), ManagerError>) -> String {
    let error = error.to_string();
    match cleanup {
        Ok(()) => error,
        Err(cleanup) => format!("{error}; transparent proxy cleanup failed: {cleanup}"),
    }
}

fn deployment_label(deployment: &Deployment) -> String {
    format!("{}@{}", deployment.core, deployment.version)
}

fn transparent_cleanup_required(document: &sempre_state::Document) -> bool {
    document.active.is_some() || document.runtime.core.is_some()
}

fn path_text(path: &std::path::Path) -> Result<&str, ManagerError> {
    path.to_str()
        .ok_or_else(|| ManagerError::NonUnicodePath(path.into()))
}

#[cfg(test)]
mod tests;
