use std::time::Duration;

use sempre_manager::RuntimeStatus;
use sempre_state::{DesiredState, Layout, Mode, RuntimeState};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    ClientError,
    local_api::LocalApi,
    runtime_args::{
        RuntimeCacheCommand, RuntimeCommand, RuntimeConfigCommand, RuntimeConnectionCommand,
        RuntimeDnsCommand, RuntimeProviderCommand, RuntimeProxyCommand, RuntimeRuleProviderCommand,
        RuntimeStreamTopic,
    },
};

const DEFAULT_DELAY_URL: &str = "https://www.gstatic.com/generate_204";
const DEFAULT_DELAY_TIMEOUT_MS: u64 = 5_000;

#[derive(Deserialize)]
struct ActionOutput {
    status: RuntimeStatus,
}

#[derive(Deserialize)]
struct ReloadOutput {
    status: RuntimeStatus,
}

pub(crate) async fn run(
    mode: Mode,
    command: RuntimeCommand,
    json: bool,
) -> Result<(), ClientError> {
    let layout = Layout::for_mode(mode)?;
    match command {
        RuntimeCommand::Status
        | RuntimeCommand::Reload
        | RuntimeCommand::Start
        | RuntimeCommand::Stop
        | RuntimeCommand::Restart => lifecycle(&layout, command, json).await,
        RuntimeCommand::Capabilities => {
            let client = LocalApi::discover(&layout.daemon_control)?;
            let value: Value = client.get("/api/v1/runtime/capabilities").await?;
            print_json(&value)
        }
        RuntimeCommand::Overview => print_json(&control(&layout)?.overview().await?),
        RuntimeCommand::Config { command } => runtime_config(&layout, command).await,
        RuntimeCommand::Proxies { command } => runtime_proxies(&layout, command).await,
        RuntimeCommand::Providers { command } => runtime_providers(&layout, command).await,
        RuntimeCommand::Rules => print_json(&control(&layout)?.rules().await?),
        RuntimeCommand::RuleProviders { command } => runtime_rule_providers(&layout, command).await,
        RuntimeCommand::Connections { command } => runtime_connections(&layout, command).await,
        RuntimeCommand::Dns { command } => runtime_dns(&layout, command).await,
        RuntimeCommand::Cache { command } => runtime_cache(&layout, command).await,
        RuntimeCommand::Events { topic } => stream(&layout, topic).await,
        RuntimeCommand::Traffic => stream(&layout, RuntimeStreamTopic::Traffic).await,
        RuntimeCommand::Memory => stream(&layout, RuntimeStreamTopic::Memory).await,
        RuntimeCommand::Logs => stream(&layout, RuntimeStreamTopic::Logs).await,
    }
}

async fn lifecycle(
    layout: &Layout,
    command: RuntimeCommand,
    json: bool,
) -> Result<(), ClientError> {
    let client = LocalApi::discover(&layout.daemon_control)?;
    let status = match command {
        RuntimeCommand::Status => client.get("/api/v1/runtime/status").await?,
        RuntimeCommand::Reload => {
            let result: ReloadOutput = client.post("/api/v1/runtime/reload").await?;
            result.status
        }
        RuntimeCommand::Start => action(&client, "start").await?,
        RuntimeCommand::Stop => action(&client, "stop").await?,
        RuntimeCommand::Restart => action(&client, "restart").await?,
        _ => unreachable!("only lifecycle commands reach lifecycle"),
    };
    print_status(&status, json)
}

fn control(layout: &Layout) -> Result<sempre_core_control::Client, ClientError> {
    Ok(sempre_core_control::Client::from_file(
        &layout.core_control,
    )?)
}

async fn runtime_config(
    layout: &Layout,
    command: Option<RuntimeConfigCommand>,
) -> Result<(), ClientError> {
    let client = control(layout)?;
    match command {
        None => print_json(&client.config().await?),
        Some(RuntimeConfigCommand::Set { key, value }) => {
            let value = serde_json::from_str(&value).map_err(ClientError::Json)?;
            client
                .patch_config(Value::Object(Map::from_iter([(key, value)])))
                .await?;
            Ok(())
        }
    }
}

async fn runtime_proxies(
    layout: &Layout,
    command: Option<RuntimeProxyCommand>,
) -> Result<(), ClientError> {
    let client = control(layout)?;
    match command {
        None => print_json(&client.proxies().await?),
        Some(RuntimeProxyCommand::Select { group, proxy }) => {
            client.select_proxy(&group, &proxy).await?;
            Ok(())
        }
        Some(RuntimeProxyCommand::Delay {
            name,
            url,
            timeout_ms,
        }) => {
            let timeout = timeout_ms.unwrap_or(DEFAULT_DELAY_TIMEOUT_MS);
            if timeout == 0 {
                return Err(ClientError::Runtime(
                    "timeout must be a positive number of milliseconds".into(),
                ));
            }
            let url = url.as_deref().unwrap_or(DEFAULT_DELAY_URL);
            let delay = client.proxy_delay(&name, url, timeout).await?;
            print_json(&json!({ "delay": delay }))
        }
    }
}

async fn runtime_providers(
    layout: &Layout,
    command: Option<RuntimeProviderCommand>,
) -> Result<(), ClientError> {
    let client = control(layout)?;
    match command {
        None => print_json(&client.providers().await?),
        Some(RuntimeProviderCommand::Update { name }) => {
            client.provider_action(&name, false).await?;
            Ok(())
        }
        Some(RuntimeProviderCommand::Healthcheck { name }) => {
            client.provider_action(&name, true).await?;
            Ok(())
        }
    }
}

async fn runtime_rule_providers(
    layout: &Layout,
    command: Option<RuntimeRuleProviderCommand>,
) -> Result<(), ClientError> {
    let client = control(layout)?;
    match command {
        None => print_json(&client.rule_providers().await?),
        Some(RuntimeRuleProviderCommand::Update { name }) => {
            client.update_rule_provider(&name).await?;
            Ok(())
        }
    }
}

async fn runtime_connections(
    layout: &Layout,
    command: Option<RuntimeConnectionCommand>,
) -> Result<(), ClientError> {
    let client = control(layout)?;
    match command {
        None => print_json(&client.connections().await?),
        Some(RuntimeConnectionCommand::Close { id, all: _ }) => {
            client.close_connection(id.as_deref().unwrap_or("")).await?;
            Ok(())
        }
    }
}

async fn runtime_dns(layout: &Layout, command: RuntimeDnsCommand) -> Result<(), ClientError> {
    let RuntimeDnsCommand::Query { name, record_type } = command;
    print_json(&control(layout)?.dns_query(&name, &record_type).await?)
}

async fn runtime_cache(layout: &Layout, command: RuntimeCacheCommand) -> Result<(), ClientError> {
    match command {
        RuntimeCacheCommand::Flush => control(layout)?.flush_fake_ip().await?,
    }
    Ok(())
}

async fn stream(layout: &Layout, topic: RuntimeStreamTopic) -> Result<(), ClientError> {
    let mut stream = control(layout)?.stream(topic.as_str()).await?;
    loop {
        println!(
            "{}",
            serde_json::to_string(&stream.next_json().await?).map_err(ClientError::Json)?
        );
    }
}

fn print_json(value: &impl serde::Serialize) -> Result<(), ClientError> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(ClientError::Json)?
    );
    Ok(())
}

async fn action(client: &LocalApi, action: &str) -> Result<RuntimeStatus, ClientError> {
    let before: RuntimeStatus = client.get("/api/v1/runtime/status").await?;
    let accepted: ActionOutput = client.post(&format!("/api/v1/runtime/{action}")).await?;
    if complete(action, &before, &accepted.status) {
        return Ok(accepted.status);
    }
    let deadline = tokio::time::Instant::now() + Duration::from_mins(1);
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let current: RuntimeStatus = client.get("/api/v1/runtime/status").await?;
        if complete(action, &before, &current) {
            return Ok(current);
        }
        if current.runtime_state == RuntimeState::Failed {
            return Err(ClientError::Runtime(
                current
                    .last_error
                    .unwrap_or_else(|| "managed core entered failed state".into()),
            ));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(ClientError::Runtime(format!(
                "timed out waiting for managed core to {action}"
            )));
        }
    }
}

fn complete(action: &str, before: &RuntimeStatus, current: &RuntimeStatus) -> bool {
    match action {
        "stop" => {
            current.desired_state == DesiredState::Stopped
                && matches!(
                    current.runtime_state,
                    RuntimeState::Stopped | RuntimeState::Idle
                )
        }
        "restart" => {
            current.runtime_state == RuntimeState::Running
                && (before.pid == 0 || current.pid != before.pid)
        }
        _ => current.runtime_state == RuntimeState::Running,
    }
}

fn print_status(status: &RuntimeStatus, json: bool) -> Result<(), ClientError> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(status).map_err(ClientError::Json)?
        );
        return Ok(());
    }
    let deployment = status.active.as_ref().or(status.target.as_ref());
    println!("Desired: {}", label(&status.desired_state)?);
    println!("State: {}", label(&status.runtime_state)?);
    println!(
        "Core: {}",
        deployment.map_or("none", |value| value.exact_reference.as_str())
    );
    println!(
        "Config: {}",
        deployment.map_or("none", |value| value.config_hash.as_str())
    );
    println!("PID: {}", status.pid);
    println!("Uptime: {}s", status.uptime_seconds);
    println!("Restarts: {}", status.restart_count);
    println!("Pending: {}", status.pending);
    if let Some(error) = &status.last_error {
        println!("Last error: {error}");
    }
    Ok(())
}

fn label(value: &impl serde::Serialize) -> Result<String, ClientError> {
    serde_json::to_value(value)
        .map_err(ClientError::Json)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ClientError::Runtime("runtime state is not a string".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sempre_manager::{RuntimeActionAvailability, RuntimeActions};

    fn status(desired: DesiredState, runtime: RuntimeState, pid: u32) -> RuntimeStatus {
        let available = || RuntimeActionAvailability {
            allowed: true,
            reason: String::new(),
        };
        RuntimeStatus {
            desired_state: desired,
            runtime_state: runtime,
            active: None,
            target: None,
            pid,
            started_at: Some(Utc::now()),
            uptime_seconds: 1,
            restart_count: 0,
            pending: false,
            pending_changes: Vec::new(),
            last_transition: None,
            last_exit: None,
            last_error: None,
            last_failure: None,
            actions: RuntimeActions {
                start: available(),
                stop: available(),
                restart: available(),
            },
        }
    }

    #[test]
    fn lifecycle_completion_requires_observed_runtime_state() {
        let before = status(DesiredState::Running, RuntimeState::Running, 41);
        let stopping = status(DesiredState::Running, RuntimeState::Stopping, 41);
        let restarted = status(DesiredState::Running, RuntimeState::Running, 42);
        assert!(!complete("restart", &before, &stopping));
        assert!(complete("restart", &before, &restarted));
        let stopped = status(DesiredState::Stopped, RuntimeState::Stopped, 0);
        assert!(complete("stop", &before, &stopped));
    }
}
