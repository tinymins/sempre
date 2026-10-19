use std::{process::ExitStatus, time::Duration};

use sempre_supervisor::{ManagedProcess, SupervisorError};
use tokio::{sync::watch, time::sleep};

use super::RuntimePlan;
use crate::{Manager, ValidationRunner, VersionRunner};

pub(super) enum ProcessEvent {
    Healthy,
    Reload,
    Shutdown,
    Exited(Result<ExitStatus, SupervisorError>),
}

pub(super) enum RetryEvent {
    Reload,
    NetworkChanged,
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
        () = activate_network(manager, plan, grace) => ProcessEvent::Healthy,
        result = process.wait() => ProcessEvent::Exited(result),
        () = manager.wait_runtime_reload() => ProcessEvent::Reload,
        () = shutdown_requested(shutdown) => ProcessEvent::Shutdown,
    }
}

async fn activate_network<R: VersionRunner + ValidationRunner>(
    manager: &Manager<R>,
    plan: &RuntimePlan,
    grace: Duration,
) {
    manager.activate_network_automation(grace).await;
    if plan.dns_frontend.is_none() && !plan.transparent.active() {
        sleep(grace).await;
    } else if let Err(error) = manager.transparent.apply(&plan.transparent).await {
        manager.log_supervisor(&format!("transparent proxy activation failed: {error}"));
    }
    if plan.dns_frontend.is_none() {
        if let Err(error) = manager.transparent.cleanup_system_dns().await {
            manager.log_supervisor(&format!("system DNS cleanup failed: {error}"));
        }
        manager.dns_frontend.stop().await;
    }
    if let Err(error) = manager
        .dns_frontend
        .activate_core(plan.dns_frontend.as_ref())
        .await
    {
        manager.log_supervisor(&format!("DNS frontend activation failed: {error}"));
    }
    if let Some(intent) = &plan.transparent.cn_routes {
        if let Err(error) = manager.transparent.flush_cn_dns().await {
            manager.log_supervisor(&format!("CN DNS flush failed: {error}"));
        }
        if let Err(error) = manager.transparent.apply_cn_routes(intent).await {
            manager.log_supervisor(&format!("CN routes activation failed: {error}"));
        }
    }
}

pub(super) async fn wait_running<R: VersionRunner + ValidationRunner>(
    manager: &Manager<R>,
    shutdown: &mut watch::Receiver<bool>,
    process: &mut ManagedProcess,
    plan: &RuntimePlan,
) -> ProcessEvent {
    tokio::select! {
        () = manager.monitor_cn_routes(plan) => ProcessEvent::Reload,
        () = manager.complete_rule_bootstrap(plan) => ProcessEvent::Reload,
        () = manager.monitor_network_automation() => unreachable!("network monitor does not exit"),
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

pub(super) async fn consume_reload<R: VersionRunner>(manager: &Manager<R>) {
    // A new cycle uses the latest intent, including any queued reload wakeup.
    tokio::select! {
        biased;
        () = manager.wait_runtime_reload() => {},
        () = std::future::ready(()) => {},
    }
}

pub(super) async fn wait_preparation<R: VersionRunner>(
    manager: &Manager<R>,
    shutdown: &mut watch::Receiver<bool>,
) -> bool {
    tokio::select! {
        () = manager.startup_tasks.wait_prepared() => false,
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
        () = crate::network_automation::wait_for_network_change() => RetryEvent::NetworkChanged,
        () = shutdown_requested(shutdown) => RetryEvent::Shutdown,
        () = sleep(delay) => RetryEvent::Timer,
    }
}

async fn shutdown_requested(shutdown: &mut watch::Receiver<bool>) {
    if !*shutdown.borrow() {
        let _ = shutdown.changed().await;
    }
}
