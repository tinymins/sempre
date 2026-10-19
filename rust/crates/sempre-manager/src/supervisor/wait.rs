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
    manager.activate_network_automation(grace).await;
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
    ensure_cn_dns_ready(&plan.transparent, &manager.dns_frontend.status())?;
    if let Some(intent) = &plan.transparent.cn_routes {
        manager.transparent.flush_cn_dns().await?;
        manager.transparent.apply_cn_routes(intent).await?;
    }
    Ok(())
}

fn ensure_cn_dns_ready(
    plan: &sempre_transparent::Plan,
    status: &crate::DnsFrontendStatus,
) -> Result<(), sempre_transparent::TransparentError> {
    if plan.cn_routes.is_some() && !(status.enabled && status.running && status.core_dns_healthy) {
        let mut message = "managed DNS frontend is not ready; CN routes remain disabled".to_owned();
        if !status.last_error.is_empty() {
            message.push_str(": ");
            message.push_str(&status.last_error);
        }
        return Err(sempre_transparent::TransparentError::Invalid(message));
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

async fn shutdown_requested(shutdown: &mut watch::Receiver<bool>) {
    if !*shutdown.borrow() {
        let _ = shutdown.changed().await;
    }
}

#[cfg(test)]
mod tests {
    use super::ensure_cn_dns_ready;
    use crate::DnsFrontendStatus;
    use sempre_transparent::{CnRouteIntent, Plan};

    #[test]
    fn cn_routes_require_running_healthy_managed_dns() {
        for cn_enabled in [false, true] {
            for mode in ["fake-ip", "real-ip"] {
                for (enabled, running, core_dns_healthy) in [
                    (true, true, true),
                    (false, true, true),
                    (true, false, true),
                    (true, true, false),
                ] {
                    let plan = Plan {
                        cn_routes: cn_enabled.then_some(CnRouteIntent {
                            direct: true,
                            prefixes: Vec::new(),
                            tun_addresses: Vec::new(),
                            capture_prefixes: Vec::new(),
                            outbound_interfaces: [None, None],
                        }),
                        ..Plan::default()
                    };
                    let status = DnsFrontendStatus {
                        enabled,
                        running,
                        core_dns_healthy,
                        mode: mode.into(),
                        last_error: "DNS probe or capture failed".into(),
                        ..DnsFrontendStatus::default()
                    };
                    let result = ensure_cn_dns_ready(&plan, &status);
                    assert_eq!(
                        result.is_ok(),
                        !cn_enabled || (enabled && running && core_dns_healthy),
                        "CN={cn_enabled} mode={mode} enabled={enabled} running={running} healthy={core_dns_healthy}"
                    );
                    if let Err(error) = result {
                        assert!(
                            error
                                .to_string()
                                .contains("CN routes remain disabled: DNS probe or capture failed")
                        );
                    }
                }
            }
        }
    }
}
