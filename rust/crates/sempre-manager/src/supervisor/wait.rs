use std::{process::ExitStatus, time::Duration};

use sempre_supervisor::{ManagedProcess, SupervisorError};
use tokio::{sync::watch, time::sleep};

use super::RuntimePlan;
use crate::{Manager, ValidationRunner, VersionRunner};

pub(super) enum ProcessEvent {
    Healthy(Result<(), sempre_transparent::TransparentError>),
    Reload,
    Shutdown,
    Exited(Result<ExitStatus, SupervisorError>),
}

pub(super) enum RetryEvent {
    Reload,
    Timer,
    Shutdown,
}

pub(super) async fn wait_startup<R: VersionRunner + ValidationRunner>(
    manager: &Manager<R>,
    shutdown: &mut watch::Receiver<bool>,
    process: &mut ManagedProcess,
    plan: &RuntimePlan,
    grace: Duration,
) -> ProcessEvent {
    tokio::select! {
        result = activate_network(manager, plan, grace) => ProcessEvent::Healthy(result),
        result = process.wait() => ProcessEvent::Exited(result),
        () = manager.wait_runtime_reload() => ProcessEvent::Reload,
        () = shutdown_requested(shutdown) => ProcessEvent::Shutdown,
    }
}

async fn activate_network<R: VersionRunner + ValidationRunner>(
    manager: &Manager<R>,
    plan: &RuntimePlan,
    grace: Duration,
) -> Result<(), sempre_transparent::TransparentError> {
    if plan.dns_frontend.is_some() {
        manager
            .dns_frontend
            .activate(plan, grace)
            .await
            .map_err(|error| sempre_transparent::TransparentError::Invalid(error.to_string()))?;
    } else if !plan.transparent.active() {
        sleep(grace).await;
        return Ok(());
    }
    manager.transparent.apply(&plan.transparent).await
}

pub(super) async fn wait_running<R: VersionRunner + ValidationRunner>(
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

pub(super) async fn wait_inactive<R: VersionRunner>(
    manager: &Manager<R>,
    shutdown: &mut watch::Receiver<bool>,
) -> bool {
    tokio::select! {
        () = manager.wait_runtime_reload() => false,
        () = shutdown_requested(shutdown) => true,
    }
}

pub(super) async fn wait_retry<R: VersionRunner>(
    manager: &Manager<R>,
    shutdown: &mut watch::Receiver<bool>,
    delay: Duration,
) -> RetryEvent {
    tokio::select! {
        () = manager.wait_runtime_reload() => RetryEvent::Reload,
        () = shutdown_requested(shutdown) => RetryEvent::Shutdown,
        () = sleep(delay) => RetryEvent::Timer,
    }
}

async fn shutdown_requested(shutdown: &mut watch::Receiver<bool>) {
    if !*shutdown.borrow() {
        let _ = shutdown.changed().await;
    }
}
