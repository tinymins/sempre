use std::{future::Future, time::Duration};

use tokio::time::{sleep, timeout};

use crate::{Manager, ManagerError, VersionRunner};

pub(crate) const LOCK_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const PREPARE_TIMEOUT: Duration = Duration::from_mins(1);
pub(super) const NETWORK_TIMEOUT: Duration = Duration::from_secs(15);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);

impl<R: VersionRunner> Manager<R> {
    pub(crate) async fn lifecycle_phase<T, E: Into<ManagerError>>(
        &self,
        stage: &str,
        description: &str,
        limit: Duration,
        operation: impl Future<Output = Result<T, E>>,
    ) -> Result<T, ManagerError> {
        self.startup_tasks.log(
            stage,
            &format!("{description}; timeout {}s", limit.as_secs()),
        );
        let result = timeout(limit, operation)
            .await
            .map_err(|_| {
                let message = format!("{description} timed out after {}s", limit.as_secs());
                self.startup_tasks.log("timeout", &message);
                ManagerError::RuntimeAction {
                    code: "RUNTIME_STAGE_TIMEOUT",
                    message,
                }
            })?
            .map_err(Into::into);
        match &result {
            Ok(_) => self
                .startup_tasks
                .log(stage, &format!("{description}: completed")),
            Err(error) => self
                .startup_tasks
                .log("error", &format!("{description}: {error}")),
        }
        result
    }

    pub(super) async fn network_phase<E: Into<ManagerError>>(
        &self,
        stage: &str,
        description: &str,
        operation: impl Future<Output = Result<(), E>>,
    ) -> Result<(), ManagerError>
    where
        R: crate::ValidationRunner,
    {
        match self
            .lifecycle_phase(stage, description, NETWORK_TIMEOUT, operation)
            .await
        {
            Err(
                error @ ManagerError::RuntimeAction {
                    code: "RUNTIME_STAGE_TIMEOUT",
                    ..
                },
            ) => Err(error),
            Err(error) => {
                self.log_supervisor(&format!("{description} failed: {error}"));
                Ok(())
            }
            Ok(()) => Ok(()),
        }
    }

    pub(crate) async fn operation_lease(&self) -> Result<sempre_state::Lease, ManagerError> {
        self.lifecycle_phase(
            "runtime_lock",
            "Waiting for configuration operation lock",
            LOCK_TIMEOUT,
            async {
                loop {
                    if let Some(lease) = self.store.try_acquire_operation()? {
                        return Ok::<_, ManagerError>(lease);
                    }
                    sleep(Duration::from_millis(50)).await;
                }
            },
        )
        .await
    }

    pub(crate) async fn cleanup_lifecycle(&self, retain_dns: bool) -> Result<(), ManagerError> {
        let dns = if retain_dns && self.dns_frontend.status().running {
            Ok(())
        } else {
            self.lifecycle_phase(
                "dns_frontend",
                "Stopping DNS frontend",
                CLEANUP_TIMEOUT,
                async {
                    self.dns_frontend.stop().await;
                    Ok::<_, ManagerError>(())
                },
            )
            .await
        };
        let gateway = self
            .lifecycle_phase("gateway", "Stopping gateway", CLEANUP_TIMEOUT, async {
                self.stop_gateway().await;
                Ok::<_, ManagerError>(())
            })
            .await;
        let network = self
            .lifecycle_phase(
                "cleanup",
                "Restoring managed network and DNS state",
                CLEANUP_TIMEOUT,
                async {
                    if retain_dns && self.dns_frontend.status().running {
                        self.transparent.cleanup_runtime_network().await
                    } else {
                        self.transparent.cleanup().await
                    }
                },
            )
            .await;
        let errors = [dns, gateway, network]
            .into_iter()
            .filter_map(Result::err)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ManagerError::RuntimeNotReady(errors.join("; ")))
        }
    }
}

impl<R: VersionRunner + crate::ValidationRunner> Manager<R> {
    pub(super) async fn fail_network_startup(
        &self,
        plan: &super::RuntimePlan,
        process: &mut sempre_supervisor::ManagedProcess,
        error: &ManagerError,
    ) -> Result<(), ManagerError> {
        process.terminate(super::STOP_GRACE).await?;
        let cleanup = self.cleanup_after_core_failure().await;
        self.remove_control();
        let error = super::with_cleanup_failure(error, cleanup);
        self.handle_process_failure(plan, "network startup failed", &error)
    }

    pub(super) async fn recover_lifecycle(&self) -> Result<sempre_state::Document, ManagerError> {
        let initial = self.store.read()?;
        if initial.desired_state == sempre_state::DesiredState::Running && initial.active.is_some()
        {
            self.startup_tasks.begin_cycle();
        }
        self.lifecycle_phase(
            "recovery",
            "Recovering stale core process",
            NETWORK_TIMEOUT,
            super::recovery::recover_stale_process(self),
        )
        .await?;
        self.network_phase(
            "recovery",
            "Recovering managed network and DNS state",
            self.transparent.recover_stale_system_dns(),
        )
        .await?;
        Ok(initial)
    }

    pub(super) async fn start_runtime_gateway(
        &self,
        plan: &super::RuntimePlan,
        shutdown: &mut tokio::sync::watch::Receiver<bool>,
    ) -> Result<Option<super::CycleResult>, ManagerError> {
        let mut cancellation = self.startup_tasks.cancellation();
        let gateway = tokio::select! {
            biased;
            _ = cancellation.changed() => {
                self.cleanup_retained_frontend().await?;
                return Ok(Some(super::CycleResult::Restart));
            },
            () = super::wait::shutdown_requested(shutdown) => {
                self.cleanup_retained_frontend().await?;
                super::state::mark_intentional_exit(self, true)?;
                return Ok(Some(super::CycleResult::Shutdown));
            },
            result = self.lifecycle_phase("gateway", "Starting gateway", NETWORK_TIMEOUT, Box::pin(self.start_gateway())) => result,
        };
        if let Err(error) = gateway {
            self.handle_process_failure(plan, "gateway startup failed", &error)?;
            return Ok(Some(super::CycleResult::Failed));
        }
        Ok(None)
    }

    pub(super) async fn prepare_cycle<'a>(
        &'a self,
        shutdown: &mut tokio::sync::watch::Receiver<bool>,
    ) -> Result<Option<(tokio::sync::MutexGuard<'a, ()>, super::RuntimePlan)>, ManagerError> {
        let mut cancellation = self.startup_tasks.cancellation();
        let prepare = async {
            let gate = self
                .lifecycle_phase(
                    "runtime_lock",
                    "Waiting for runtime configuration lock",
                    LOCK_TIMEOUT,
                    async { Ok::<_, ManagerError>(self.runtime_rules_gate.lock().await) },
                )
                .await?;
            super::wait::consume_reload(self).await;
            if self.store.read()?.desired_state == sempre_state::DesiredState::Stopped {
                return Ok(None);
            }
            let plan = self
                .lifecycle_phase(
                    "preparing",
                    "Preparing core runtime configuration",
                    PREPARE_TIMEOUT,
                    Box::pin(self.resolve_runtime_plan()),
                )
                .await?;
            Ok(Some((gate, plan)))
        };
        tokio::select! {
            biased;
            _ = cancellation.changed() => Ok(None),
            () = super::wait::shutdown_requested(shutdown) => Ok(None),
            result = prepare => result,
        }
    }
}
