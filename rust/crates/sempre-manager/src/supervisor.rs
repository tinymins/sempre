mod state;

use std::{fs, path::PathBuf, process::ExitStatus, time::Duration};

use chrono::Utc;
use sempre_core::{CommandSpec, ControlSpec, CoreRef};
use sempre_state::{Deployment, DesiredState};
use sempre_supervisor::{ManagedProcess, SupervisorError, append_log};
use sempre_transparent::Plan as TransparentPlan;
use sha2::{Digest, Sha256};
use tokio::{sync::watch, time::sleep};

use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

const STARTUP_GRACE: Duration = Duration::from_secs(10);
const STOP_GRACE: Duration = Duration::from_secs(10);
const MAX_BACKOFF: Duration = Duration::from_mins(1);

struct RuntimePlan {
    deployment: Deployment,
    spec: CommandSpec,
    runtime_config: PathBuf,
    runtime_config_hash: String,
    control: Option<ControlSpec>,
    transparent: TransparentPlan,
}

enum ProcessEvent {
    Healthy(Result<(), sempre_transparent::TransparentError>),
    Reload,
    Shutdown,
    Exited(Result<ExitStatus, SupervisorError>),
}

enum CycleResult {
    Restart,
    Failed { retry_immediately: bool },
    Shutdown,
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub async fn run_supervisor(
        &self,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), ManagerError> {
        self.run_supervisor_with_grace(shutdown, STARTUP_GRACE)
            .await
    }

    async fn run_supervisor_with_grace(
        &self,
        mut shutdown: watch::Receiver<bool>,
        startup_grace: Duration,
    ) -> Result<(), ManagerError> {
        let mut backoff = Duration::from_secs(1);
        loop {
            if *shutdown.borrow() {
                self.stop_gateway().await;
                self.transparent.cleanup().await?;
                state::mark_intentional_exit(self, true)?;
                return Ok(());
            }
            let document = self.store.read()?;
            if document.desired_state == DesiredState::Stopped {
                self.stop_gateway().await;
                self.transparent.cleanup().await?;
                state::mark_inactive(self, true)?;
                if wait_inactive(self, &mut shutdown).await {
                    return Ok(());
                }
                backoff = Duration::from_secs(1);
                continue;
            }
            if document.active.is_none() {
                self.stop_gateway().await;
                self.transparent.cleanup().await?;
                state::mark_inactive(self, false)?;
                if wait_inactive(self, &mut shutdown).await {
                    return Ok(());
                }
                backoff = Duration::from_secs(1);
                continue;
            }
            let plan = match self.resolve_runtime_plan().await {
                Ok(plan) => plan,
                Err(error) => {
                    self.stop_gateway().await;
                    self.log_supervisor(&format!("resolve deployment failed: {error}"))?;
                    if state::record_failure(
                        self,
                        "resolve failed",
                        &error.to_string(),
                        true,
                        false,
                    )? {
                        backoff = Duration::from_secs(1);
                        continue;
                    }
                    if wait_retry(self, &mut shutdown, backoff).await {
                        return Ok(());
                    }
                    backoff = next_backoff(backoff);
                    continue;
                }
            };
            if !state::mark_starting(self, &plan)? {
                continue;
            }
            match self
                .run_runtime_plan(&plan, &mut shutdown, startup_grace)
                .await?
            {
                CycleResult::Restart => backoff = Duration::from_secs(1),
                CycleResult::Shutdown => return Ok(()),
                CycleResult::Failed { retry_immediately } => {
                    if retry_immediately {
                        backoff = Duration::from_secs(1);
                    } else {
                        if wait_retry(self, &mut shutdown, backoff).await {
                            return Ok(());
                        }
                        backoff = next_backoff(backoff);
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
    ) -> Result<CycleResult, ManagerError> {
        self.log_supervisor(&format!("starting {}", deployment_label(&plan.deployment)))?;
        if let Err(error) = self.start_gateway().await {
            let retry =
                self.handle_process_failure(plan, "gateway startup failed", &error, true)?;
            return Ok(CycleResult::Failed {
                retry_immediately: retry,
            });
        }
        let mut process = match ManagedProcess::spawn(
            &plan.spec,
            &self.store.layout().core_stdout_log,
            &self.store.layout().core_stderr_log,
        ) {
            Ok(process) => process,
            Err(error) => {
                self.stop_gateway().await;
                let error = with_cleanup_failure(&error, self.transparent.cleanup().await);
                let retry = self.handle_process_failure(plan, "startup failed", &error, true)?;
                return Ok(CycleResult::Failed {
                    retry_immediately: retry,
                });
            }
        };
        let setup = self.mark_runtime_started(plan, process.pid());
        if let Err(error) = setup {
            let _ = process.terminate(STOP_GRACE).await;
            self.stop_gateway().await;
            if let Err(cleanup) = self.transparent.cleanup().await {
                self.log_supervisor(&format!("transparent proxy cleanup failed: {cleanup}"))?;
            }
            self.remove_control();
            return Err(error);
        }

        match wait_startup(self, shutdown, &mut process, plan, startup_grace).await {
            ProcessEvent::Healthy(Ok(())) => {
                if let Err(error) = self.mark_runtime_healthy(plan) {
                    let _ = process.terminate(STOP_GRACE).await;
                    if let Err(cleanup) = self.transparent.cleanup().await {
                        self.log_supervisor(&format!(
                            "transparent proxy cleanup failed: {cleanup}"
                        ))?;
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
                self.stop_process(&mut process, false).await?;
                return Ok(CycleResult::Restart);
            }
            ProcessEvent::Shutdown => {
                self.stop_process(&mut process, true).await?;
                return Ok(CycleResult::Shutdown);
            }
            ProcessEvent::Exited(result) => {
                self.stop_gateway().await;
                let cleanup = self.transparent.cleanup().await;
                self.remove_control();
                let exit = exit_result(result);
                let error = with_cleanup_failure(&exit, cleanup);
                let retry = self.handle_process_failure(plan, "startup failed", &error, true)?;
                return Ok(CycleResult::Failed {
                    retry_immediately: retry,
                });
            }
        }

        match wait_running(self, shutdown, &mut process).await {
            ProcessEvent::Reload => {
                self.stop_process(&mut process, false).await?;
                Ok(CycleResult::Restart)
            }
            ProcessEvent::Shutdown => {
                self.stop_process(&mut process, true).await?;
                Ok(CycleResult::Shutdown)
            }
            ProcessEvent::Exited(result) => {
                self.stop_gateway().await;
                let cleanup = self.transparent.cleanup().await;
                self.remove_control();
                let exit = exit_result(result);
                let error = with_cleanup_failure(&exit, cleanup);
                self.handle_process_failure(plan, "core exited", &error, false)?;
                Ok(CycleResult::Failed {
                    retry_immediately: false,
                })
            }
            ProcessEvent::Healthy(_) => unreachable!("running process has no startup timer"),
        }
    }

    async fn resolve_runtime_plan(&self) -> Result<RuntimePlan, ManagerError> {
        let document = self.store.read()?;
        let deployment = document
            .active
            .ok_or_else(|| ManagerError::RuntimeNotReady("no active core deployment".into()))?;
        if document.desired_state == DesiredState::Stopped {
            return Err(ManagerError::RuntimeNotReady(
                "managed core is stopped".into(),
            ));
        }
        let adapter = self.registry.get(&deployment.core)?;
        let binary = self.store.layout().core_binary(
            &deployment.core,
            deployment.repository.as_deref(),
            &deployment.version,
        );
        let config = self
            .store
            .layout()
            .config(&deployment.core, &deployment.config_hash);
        if !binary.is_file() || !config.is_file() {
            return Err(ManagerError::RuntimeNotReady(
                "active core binary or configuration is unavailable".into(),
            ));
        }
        let data = self.store.layout().runtime.join(&deployment.core);
        fs::create_dir_all(&data)
            .map_err(|error| ManagerError::io("create core runtime directory", error))?;
        let control_directory = data.join("control");
        if control_directory.exists() {
            fs::remove_dir_all(&control_directory)
                .map_err(|error| ManagerError::io("reset core control directory", error))?;
        }
        let runtime = adapter.prepare_runtime(&config, &control_directory)?;
        let transparent = if let Some(profile_id) = document.active_profile_id.as_deref() {
            let catalog = self.subscriptions.read()?;
            let profile = catalog
                .profiles
                .iter()
                .find(|profile| profile.id == profile_id)
                .ok_or_else(|| ManagerError::ProfileNotFound(profile_id.into()))?;
            self.transparent
                .prepare(&deployment.core, profile, &runtime.config)?
        } else {
            TransparentPlan::default()
        };
        let reference = CoreRef {
            core: deployment.core.clone(),
            repository: deployment.repository.clone(),
            reference: deployment.reference.clone(),
        };
        self.validate_config_path(&reference, &deployment.version, &runtime.config)
            .await?;
        let runtime_data = fs::read(&runtime.config)
            .map_err(|error| ManagerError::io("read runtime configuration", error))?;
        let runtime_config_hash = format!("{:x}", Sha256::digest(runtime_data));
        let binary = path_text(&binary)?;
        let runtime_config = path_text(&runtime.config)?;
        let data_text = path_text(&data)?;
        Ok(RuntimePlan {
            spec: adapter.run_spec(binary, runtime_config, data_text),
            deployment,
            runtime_config: runtime.config,
            runtime_config_hash,
            control: runtime.control,
            transparent,
        })
    }

    async fn stop_process(
        &self,
        process: &mut ManagedProcess,
        service_stopped: bool,
    ) -> Result<(), ManagerError> {
        let transition = state::mark_stopping(self);
        self.stop_gateway().await;
        let transparent = self.transparent.cleanup().await;
        let terminated = process.terminate(STOP_GRACE).await;
        self.remove_control();
        transition?;
        let state = state::mark_intentional_exit(self, service_stopped);
        transparent?;
        terminated?;
        state
    }

    fn mark_runtime_healthy(&self, plan: &RuntimePlan) -> Result<(), ManagerError> {
        state::mark_healthy(self, plan).and_then(|()| {
            self.log_supervisor(&format!("healthy {}", deployment_label(&plan.deployment)))
        })
    }

    fn mark_runtime_started(&self, plan: &RuntimePlan, pid: u32) -> Result<(), ManagerError> {
        state::mark_started(self, plan, pid)
            .and_then(|()| self.write_control(plan.control.as_ref()))
            .and_then(|()| {
                self.log_supervisor(&format!(
                    "started {} with PID {pid}",
                    deployment_label(&plan.deployment)
                ))
            })
    }

    async fn fail_transparent_startup(
        &self,
        plan: &RuntimePlan,
        process: &mut ManagedProcess,
        error: &sempre_transparent::TransparentError,
    ) -> Result<CycleResult, ManagerError> {
        let _ = process.terminate(STOP_GRACE).await;
        self.stop_gateway().await;
        let error = with_cleanup_failure(error, self.transparent.cleanup().await);
        self.remove_control();
        let retry =
            self.handle_process_failure(plan, "transparent proxy startup failed", &error, true)?;
        Ok(CycleResult::Failed {
            retry_immediately: retry,
        })
    }

    fn handle_process_failure(
        &self,
        plan: &RuntimePlan,
        stage: &str,
        error: &impl ToString,
        rollback: bool,
    ) -> Result<bool, ManagerError> {
        let message = error.to_string();
        self.log_supervisor(&format!(
            "{stage} for {}: {message}",
            deployment_label(&plan.deployment)
        ))?;
        state::record_failure(self, stage, &message, rollback, true)
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

    pub(crate) fn log_supervisor(&self, message: &str) -> Result<(), ManagerError> {
        let line = format!("{} {message}\n", Utc::now().to_rfc3339());
        append_log(&self.store.layout().manager_log, &line)?;
        Ok(())
    }
}

async fn wait_startup<R: VersionRunner + ValidationRunner>(
    manager: &Manager<R>,
    shutdown: &mut watch::Receiver<bool>,
    process: &mut ManagedProcess,
    plan: &RuntimePlan,
    grace: Duration,
) -> ProcessEvent {
    if plan.transparent.active() {
        return tokio::select! {
            result = manager.transparent.apply(&plan.transparent) => ProcessEvent::Healthy(result),
            result = process.wait() => ProcessEvent::Exited(result),
            () = manager.wait_runtime_reload() => ProcessEvent::Reload,
            () = shutdown_requested(shutdown) => ProcessEvent::Shutdown,
        };
    }
    tokio::select! {
        result = process.wait() => ProcessEvent::Exited(result),
        () = manager.wait_runtime_reload() => ProcessEvent::Reload,
        () = shutdown_requested(shutdown) => ProcessEvent::Shutdown,
        () = sleep(grace) => ProcessEvent::Healthy(Ok(())),
    }
}

async fn wait_running<R: VersionRunner + ValidationRunner>(
    manager: &Manager<R>,
    shutdown: &mut watch::Receiver<bool>,
    process: &mut ManagedProcess,
) -> ProcessEvent {
    tokio::select! {
        result = process.wait() => ProcessEvent::Exited(result),
        () = manager.wait_runtime_reload() => ProcessEvent::Reload,
        () = shutdown_requested(shutdown) => ProcessEvent::Shutdown,
    }
}

async fn wait_inactive<R: VersionRunner>(
    manager: &Manager<R>,
    shutdown: &mut watch::Receiver<bool>,
) -> bool {
    tokio::select! {
        () = manager.wait_runtime_reload() => false,
        () = shutdown_requested(shutdown) => true,
    }
}

async fn wait_retry<R: VersionRunner>(
    manager: &Manager<R>,
    shutdown: &mut watch::Receiver<bool>,
    delay: Duration,
) -> bool {
    tokio::select! {
        () = manager.wait_runtime_reload() => false,
        () = shutdown_requested(shutdown) => true,
        () = sleep(delay) => false,
    }
}

async fn shutdown_requested(shutdown: &mut watch::Receiver<bool>) {
    if !*shutdown.borrow() {
        let _ = shutdown.changed().await;
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

fn next_backoff(current: Duration) -> Duration {
    current.saturating_mul(2).min(MAX_BACKOFF)
}

fn deployment_label(deployment: &Deployment) -> String {
    format!("{}@{}", deployment.core, deployment.version)
}

fn path_text(path: &std::path::Path) -> Result<&str, ManagerError> {
    path.to_str()
        .ok_or_else(|| ManagerError::NonUnicodePath(path.into()))
}

#[cfg(test)]
mod tests;
