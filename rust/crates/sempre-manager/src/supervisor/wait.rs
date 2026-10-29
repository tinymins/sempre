use std::{process::ExitStatus, time::Duration};

use sempre_supervisor::{ManagedProcess, SupervisorError};
use tokio::{sync::watch, time::sleep};

use super::RuntimePlan;
use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

pub(super) enum ProcessEvent {
    Healthy,
    StartupFailed(ManagerError),
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
        result = activate_network(manager, plan, grace) => match result {
            Ok(()) => ProcessEvent::Healthy,
            Err(error) => ProcessEvent::StartupFailed(error),
        },
        result = process.wait() => ProcessEvent::Exited(result),
        () = manager.wait_runtime_reload() => ProcessEvent::Reload,
        () = shutdown_requested(shutdown) => ProcessEvent::Shutdown,
    }
}

async fn activate_network<R: VersionRunner + ValidationRunner>(
    manager: &Manager<R>,
    plan: &RuntimePlan,
    grace: Duration,
) -> Result<(), ManagerError> {
    manager
        .network_phase(
            "network_automation",
            "Selecting network automation mode",
            async {
                manager.activate_network_automation(grace).await;
                Ok::<_, ManagerError>(())
            },
        )
        .await?;
    if plan.dns_frontend.is_none() && !plan.transparent.active() {
        manager
            .lifecycle_phase(
                "health_check",
                "Observing core startup",
                grace + Duration::from_secs(1),
                async {
                    sleep(grace).await;
                    Ok::<_, ManagerError>(())
                },
            )
            .await?;
    } else {
        manager
            .network_phase(
                "transparent_network",
                "Activating transparent network",
                manager.transparent.apply(&plan.transparent),
            )
            .await?;
    }
    if plan.dns_frontend.is_none() {
        manager
            .network_phase(
                "system_dns",
                "Restoring system DNS",
                manager.transparent.cleanup_system_dns(),
            )
            .await?;
        manager
            .network_phase("dns_frontend", "Stopping previous DNS frontend", async {
                manager.dns_frontend.stop().await;
                Ok::<_, ManagerError>(())
            })
            .await?;
    }
    manager
        .network_phase(
            "dns_frontend",
            "Activating DNS frontend",
            manager
                .dns_frontend
                .activate_core(plan.dns_frontend.as_ref()),
        )
        .await?;
    if let Some(intent) = &plan.transparent.cn_routes {
        manager
            .network_phase(
                "system_dns",
                "Flushing CN DNS cache",
                manager.transparent.flush_cn_dns(),
            )
            .await?;
        let started = std::time::Instant::now();
        let mut report = sempre_transparent::CnRouteReport::default();
        manager
            .route_phase(
                &format!(
                    "Reconciling managed CN routes ({} prefixes)",
                    intent.prefixes.len()
                ),
                super::lifecycle::ROUTE_STALL_TIMEOUT,
                async {
                    // A new core may recreate the TUN with the same identity after
                    // the OS dropped its routes; never trust the previous core's check.
                    manager.transparent.invalidate_cn_routes().await;
                    report = manager.transparent.apply_cn_routes(intent).await?;
                    Ok::<_, sempre_transparent::TransparentError>(())
                },
            )
            .await?;
        manager.log_supervisor(&format!(
            "CN routes verified for new core: {} expected, {} restored, {} removed ({} ms)",
            report.expected,
            report.added,
            report.removed,
            started.elapsed().as_millis()
        ));
    } else {
        manager
            .route_phase(
                "Removing managed CN routes",
                super::lifecycle::ROUTE_STALL_TIMEOUT,
                manager.transparent.cleanup_cn_routes(),
            )
            .await?;
    }
    Ok(())
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

pub(super) async fn shutdown_requested(shutdown: &mut watch::Receiver<bool>) {
    if !*shutdown.borrow() {
        let _ = shutdown.changed().await;
    }
}
