use std::{collections::BTreeSet, fs, path::Path, sync::Arc, time::Instant};

use ipnet::IpNet;
use sempre_core_control::Client;
use sempre_manager::Manager;
use sempre_state::RuntimeState;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::{
    DebugEvent, millis, send_error, send_failed, send_running, send_skipped, send_succeeded,
};
use crate::api::AppState;

const PRIMARY_TEST_URL: &str = "https://www.gstatic.com/generate_204";

#[derive(Debug, Eq, PartialEq)]
struct RouteScope {
    allowed_ips: Vec<String>,
    has_default_route: bool,
}

pub(super) async fn delay(manager: &Manager, client: &Client, node: &str) -> Result<Value, String> {
    let scope = inspect_route_scope(manager, node)?;
    if !scope.has_default_route {
        return Ok(json!({
            "skipped": true,
            "reason": "WireGuard is limited to private routes; no public URL-test was sent",
            "allowed_ips": scope.allowed_ips,
        }));
    }
    client
        .proxy_delay(node, PRIMARY_TEST_URL, 5000)
        .await
        .map(|delay| json!({"delay":delay,"target":PRIMARY_TEST_URL}))
        .map_err(|error| error.to_string())
}

pub(super) async fn run_debug(
    state: Arc<AppState>,
    client: Client,
    node: String,
    sender: mpsc::Sender<DebugEvent>,
) {
    let total = Instant::now();
    if !send_running(&sender, "prepare", "复用当前 WireGuard endpoint").await {
        return;
    }
    let prepared = Instant::now();
    let scope = match inspect_route_scope(&state.manager, &node) {
        Ok(scope) => scope,
        Err(error) => {
            send_failed(
                &sender,
                "prepare",
                "复用当前 WireGuard endpoint",
                prepared,
                &error,
            )
            .await;
            send_error(&sender, error).await;
            return;
        }
    };
    if !send_succeeded(
        &sender,
        "prepare",
        "复用当前 WireGuard endpoint",
        prepared,
        json!({
            "node": node,
            "route_scope": if scope.has_default_route { "default" } else { "private" },
            "allowed_ips": scope.allowed_ips,
        }),
    )
    .await
    {
        return;
    }

    if scope.has_default_route {
        run_default_route_debug(&sender, &client, &node).await;
    } else {
        run_private_route_debug(&sender, &scope).await;
    }

    let _ = sender
        .send(DebugEvent {
            event: "done",
            payload: json!({"node":node,"duration_ms":millis(total)}),
        })
        .await;
}

async fn run_default_route_debug(sender: &mpsc::Sender<DebugEvent>, client: &Client, node: &str) {
    run_url_test_step(sender, client, node, "probe", "节点探活", PRIMARY_TEST_URL).await;
    let body_reason = "sing-box URL-test only returns latency and does not expose the response body required for IP and ASN detection";
    if !send_skipped(
        sender,
        "domestic-ip",
        "国内出口 IP",
        body_reason,
        Value::Null,
    )
    .await
    {
        return;
    }
    if !send_skipped(
        sender,
        "foreign-ip",
        "国外出口 IP",
        body_reason,
        Value::Null,
    )
    .await
    {
        return;
    }
    run_url_test_step(
        sender,
        client,
        node,
        "http-baidu",
        "HTTPS · www.baidu.com",
        "https://www.baidu.com/",
    )
    .await;
    run_url_test_step(
        sender,
        client,
        node,
        "http-google",
        "HTTPS · www.google.com",
        "https://www.google.com/generate_204",
    )
    .await;
}

async fn run_private_route_debug(sender: &mpsc::Sender<DebugEvent>, scope: &RouteScope) {
    let reason = "This WireGuard endpoint has private-only AllowedIPs and no HTTPS private health target is configured; no test traffic was sent";
    let _ = send_skipped(
        sender,
        "private-probe",
        "私网连通",
        reason,
        json!({"allowed_ips":scope.allowed_ips}),
    )
    .await;
    let public_reason = "Private WireGuard routing is not a public internet exit; public IP and ASN detection do not apply";
    let _ = send_skipped(
        sender,
        "public-ip",
        "公网出口 IP",
        public_reason,
        Value::Null,
    )
    .await;
}

async fn run_url_test_step(
    sender: &mpsc::Sender<DebugEvent>,
    client: &Client,
    node: &str,
    id: &str,
    label: &str,
    url: &str,
) {
    if !send_running(sender, id, label).await {
        return;
    }
    let started = Instant::now();
    match client.proxy_delay(node, url, 5000).await {
        Ok(delay) => {
            send_succeeded(
                sender,
                id,
                label,
                started,
                json!({"url":url,"delay":delay,"method":"HEAD"}),
            )
            .await;
        }
        Err(error) => send_failed(sender, id, label, started, &error.to_string()).await,
    }
}

fn inspect_route_scope(manager: &Manager, node: &str) -> Result<RouteScope, String> {
    let document = manager.state().map_err(|error| error.to_string())?;
    if document.runtime.state != RuntimeState::Running {
        return Err("managed core is not running".into());
    }
    if document.runtime.core.as_deref() != Some("sing-box") {
        return Err("WireGuard endpoint URL-test requires a running sing-box core".into());
    }
    let config_path = document
        .runtime
        .runtime_config
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "runtime configuration is unavailable".to_string())?;
    inspect_config(Path::new(config_path), node)
}

fn inspect_config(path: &Path, node: &str) -> Result<RouteScope, String> {
    let text =
        fs::read_to_string(path).map_err(|error| format!("read runtime configuration: {error}"))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("parse sing-box configuration: {error}"))?;
    let endpoint = value
        .get("endpoints")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|endpoint| endpoint.get("tag").and_then(Value::as_str) == Some(node))
        .ok_or_else(|| {
            format!("WireGuard endpoint {node:?} is not in the running configuration")
        })?;
    if !endpoint
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.eq_ignore_ascii_case("wireguard"))
    {
        return Err(format!("endpoint {node:?} is not WireGuard"));
    }
    let mut allowed_ips = BTreeSet::new();
    let mut has_default_route = false;
    for value in endpoint
        .get("peers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|peer| {
            peer.get("allowed_ips")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(Value::as_str)
    {
        let network = value
            .parse::<IpNet>()
            .map_err(|error| format!("invalid WireGuard AllowedIP {value:?}: {error}"))?;
        has_default_route |= network.prefix_len() == 0;
        allowed_ips.insert(network.to_string());
    }
    if allowed_ips.is_empty() {
        return Err(format!("WireGuard endpoint {node:?} has no AllowedIPs"));
    }
    Ok(RouteScope {
        allowed_ips: allowed_ips.into_iter().collect(),
        has_default_route,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    fn inspect(value: &Value, node: &str) -> Result<RouteScope, String> {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{value}").unwrap();
        inspect_config(file.path(), node)
    }

    #[test]
    fn classifies_private_only_wireguard_routes() {
        let scope = inspect(
            &json!({"endpoints":[{"type":"wireguard","tag":"home","peers":[{"allowed_ips":["10.19.93.0/24","10.19.94.0/24"]}]}]}),
            "home",
        )
        .unwrap();
        assert_eq!(
            scope,
            RouteScope {
                allowed_ips: vec!["10.19.93.0/24".into(), "10.19.94.0/24".into()],
                has_default_route: false,
            }
        );
    }

    #[test]
    fn recognizes_ipv4_or_ipv6_default_routes() {
        for allowed_ip in ["0.0.0.0/0", "::/0"] {
            let scope = inspect(
                &json!({"endpoints":[{"type":"wireguard","tag":"exit","peers":[{"allowed_ips":[allowed_ip]}]}]}),
                "exit",
            )
            .unwrap();
            assert!(scope.has_default_route, "{allowed_ip}");
        }
    }

    #[test]
    fn rejects_a_wireguard_endpoint_missing_from_the_running_config() {
        let error = inspect(&json!({"endpoints":[]}), "missing").unwrap_err();
        assert_eq!(
            error,
            "WireGuard endpoint \"missing\" is not in the running configuration"
        );
    }
}
