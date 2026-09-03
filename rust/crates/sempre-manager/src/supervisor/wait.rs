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
    if plan.dns_frontend.is_none() && !plan.transparent.active() {
        sleep(grace).await;
    } else {
        manager.transparent.apply(&plan.transparent).await?;
    }
    if plan.dns_frontend.is_none() {
        manager.transparent.cleanup_system_dns().await?;
        manager.dns_frontend.stop().await;
        return Ok(());
    }
    manager
        .dns_frontend
        .activate_core(plan.dns_frontend.as_ref(), grace)
        .await;
    Ok(())
}

pub(super) async fn wait_running<R: VersionRunner + ValidationRunner>(
    manager: &Manager<R>,
    shutdown: &mut watch::Receiver<bool>,
    process: &mut ManagedProcess,
    plan: &RuntimePlan,
) -> ProcessEvent {
    tokio::select! {
        () = manager.complete_rule_bootstrap(plan) => ProcessEvent::Reload,
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
