use std::time::Duration;

use sempre_state::DesiredState;
use tokio::sync::{MutexGuard, watch};

use super::{
    CycleResult, RETRY_INTERVAL, RuntimePlan, state, transparent_cleanup_required,
    wait::{RetryEvent, wait_inactive, wait_preparation, wait_retry},
    with_cleanup_failure,
};
use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

/// A state persistence failure, the only error that ends the supervisor.
/// Runtime failures are recorded and retried so the control plane stays online.
/// There is deliberately no `From<ManagerError>`: every fatal site is explicit.
struct Fatal(ManagerError);

fn fatal<T, E: Into<ManagerError>>(result: Result<T, E>) -> Result<T, Fatal> {
    result.map_err(|error| Fatal(error.into()))
}

enum Phase<'a> {
    Recover,
    Decide,
    Inactive,
    AwaitPreparation,
    Prepare,
    Run(Box<RuntimePlan>, MutexGuard<'a, ()>),
    Backoff(Resume, ReloadCleanup),
    Stop,
    Done(Result<(), ManagerError>),
}

/// The phase a retry returns to: a core never starts before recovery completes.
#[derive(Clone, Copy)]
enum Resume {
    Recover,
    Cycle,
}

#[derive(Clone, Copy)]
enum ReloadCleanup {
    None,
    CoreFailure,
    Frontend,
}

struct Machine<'a, R: VersionRunner + ValidationRunner> {
    manager: &'a Manager<R>,
    shutdown: watch::Receiver<bool>,
    startup_grace: Duration,
    cleanup_needed: bool,
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub(super) async fn run_supervisor_machine(
        &self,
        shutdown: watch::Receiver<bool>,
        startup_grace: Duration,
    ) -> Result<(), ManagerError> {
        let mut machine = Machine {
            manager: self,
            shutdown,
            startup_grace,
            // Recovery decides; until then a stop restores host state.
            cleanup_needed: true,
        };
        let mut phase = Phase::Recover;
        loop {
            phase = match machine.step(phase).await {
                Ok(Phase::Done(result)) => return result,
                Ok(next) => next,
                Err(Fatal(error)) => return Err(error),
            };
        }
    }
}

impl<'a, R: VersionRunner + ValidationRunner> Machine<'a, R> {
    async fn step(&mut self, phase: Phase<'a>) -> Result<Phase<'a>, Fatal> {
        match phase {
            Phase::Recover => self.recover().await,
            Phase::Decide => self.decide(),
            Phase::Inactive => self.inactive().await,
            Phase::AwaitPreparation => Ok(
                if wait_preparation(self.manager, &mut self.shutdown).await {
                    Phase::Done(Ok(()))
                } else {
                    Phase::Decide
                },
            ),
            Phase::Prepare => self.prepare().await,
            Phase::Run(plan, gate) => self.run(&plan, gate).await,
            Phase::Backoff(resume, reload) => Ok(self.backoff(resume, reload).await),
            Phase::Stop => self.stop().await,
            Phase::Done(result) => Ok(Phase::Done(result)),
        }
    }

    async fn recover(&mut self) -> Result<Phase<'a>, Fatal> {
        let manager = self.manager;
        match manager.recover_lifecycle().await {
            Ok(initial) => {
                self.cleanup_needed = transparent_cleanup_required(&initial);
                Ok(Phase::Decide)
            }
            Err(error) => {
                manager.log_supervisor(&format!("runtime recovery failed: {error}"));
                fatal(state::record_failure(
                    manager,
                    "recovery failed",
                    &error.to_string(),
                    false,
                ))?;
                Ok(Phase::Backoff(Resume::Recover, ReloadCleanup::None))
            }
        }
    }

    fn decide(&mut self) -> Result<Phase<'a>, Fatal> {
        let document = fatal(self.manager.store.read())?;
        if *self.shutdown.borrow() {
            return Ok(Phase::Stop);
        }
        if document.desired_state == DesiredState::Stopped || document.active.is_none() {
            return Ok(Phase::Inactive);
        }
        if !self.manager.startup_tasks.begin_cycle() {
            return Ok(Phase::AwaitPreparation);
        }
        self.cleanup_needed = true;
        Ok(Phase::Prepare)
    }

    async fn inactive(&mut self) -> Result<Phase<'a>, Fatal> {
        let manager = self.manager;
        let stopped = fatal(manager.store.read())?.desired_state == DesiredState::Stopped;
        let cleanup = manager.cleanup_inactive(self.cleanup_needed).await;
        self.cleanup_needed = false;
        fatal(state::mark_inactive(manager, stopped))?;
        manager
            .startup_tasks
            .complete_stop(cleanup.as_ref().err().map(ToString::to_string).as_deref());
        if let Err(error) = cleanup {
            manager.log_supervisor(&format!("inactive cleanup failed: {error}"));
        }
        Ok(if wait_inactive(manager, &mut self.shutdown).await {
            Phase::Done(Ok(()))
        } else {
            Phase::Decide
        })
    }

    async fn prepare(&mut self) -> Result<Phase<'a>, Fatal> {
        let manager = self.manager;
        match manager.prepare_cycle(&mut self.shutdown).await {
            Ok(Some((gate, plan))) => {
                if fatal(state::mark_starting(manager, &plan))? {
                    Ok(Phase::Run(Box::new(plan), gate))
                } else {
                    manager.startup_tasks.interrupt_cycle();
                    Ok(Phase::Decide)
                }
            }
            Ok(None) => Ok(Phase::Decide),
            Err(error) => {
                let error =
                    with_cleanup_failure(&error, manager.cleanup_after_core_failure().await);
                manager.log_supervisor(&format!("resolve deployment failed: {error}"));
                fatal(state::record_failure(
                    manager,
                    "resolve failed",
                    &error,
                    false,
                ))?;
                Ok(Phase::Backoff(Resume::Cycle, ReloadCleanup::CoreFailure))
            }
        }
    }

    async fn run(
        &mut self,
        plan: &RuntimePlan,
        gate: MutexGuard<'a, ()>,
    ) -> Result<Phase<'a>, Fatal> {
        let manager = self.manager;
        let result = manager
            .run_runtime_plan(plan, &mut self.shutdown, self.startup_grace, gate)
            .await;
        match result {
            Ok(CycleResult::Restart) => {
                self.cleanup_needed = false;
                Ok(Phase::Decide)
            }
            Ok(CycleResult::Shutdown) => Ok(Phase::Done(Ok(()))),
            Ok(CycleResult::Failed) => Ok(Phase::Backoff(Resume::Cycle, ReloadCleanup::Frontend)),
            Err(error) => {
                fatal(manager.handle_process_failure(plan, "runtime failed", &error))?;
                Ok(Phase::Backoff(Resume::Cycle, ReloadCleanup::Frontend))
            }
        }
    }

    async fn backoff(&mut self, resume: Resume, reload: ReloadCleanup) -> Phase<'a> {
        let manager = self.manager;
        match wait_retry(manager, &mut self.shutdown, RETRY_INTERVAL).await {
            RetryEvent::Timer => {}
            RetryEvent::NetworkChanged => {
                manager.log_supervisor(match resume {
                    Resume::Recover => "network changed; retrying recovery immediately",
                    Resume::Cycle => "network changed; retrying core immediately",
                });
            }
            RetryEvent::Reload => {
                let cleanup = match reload {
                    ReloadCleanup::None => Ok(()),
                    ReloadCleanup::CoreFailure => manager.cleanup_after_core_failure().await,
                    ReloadCleanup::Frontend => manager.cleanup_retained_frontend().await,
                };
                if let Err(error) = cleanup {
                    manager.log_supervisor(&format!("reload cleanup failed: {error}"));
                }
            }
            RetryEvent::Shutdown => return Phase::Stop,
        }
        match resume {
            Resume::Recover => Phase::Recover,
            Resume::Cycle => Phase::Decide,
        }
    }

    async fn stop(&mut self) -> Result<Phase<'a>, Fatal> {
        let manager = self.manager;
        let cleanup = if self.cleanup_needed {
            manager.cleanup_retained_frontend().await
        } else {
            Ok(())
        };
        fatal(state::mark_intentional_exit(manager, true))?;
        manager
            .startup_tasks
            .complete_stop(cleanup.as_ref().err().map(ToString::to_string).as_deref());
        Ok(Phase::Done(cleanup))
    }
}
