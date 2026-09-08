use std::{collections::HashSet, sync::Arc, time::Instant};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{
    api::{AppState, api_error},
    node_diagnostic::DiagnosticCore,
};

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/runtime/nodes", get(nodes))
        .route("/api/v1/runtime/nodes/debug", post(debug_node))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct Node {
    name: String,
    #[serde(rename = "type")]
    node_type: String,
}

#[derive(Deserialize)]
struct DebugInput {
    name: String,
}

struct DebugEvent {
    event: &'static str,
    payload: Value,
}

async fn nodes(State(state): State<Arc<AppState>>) -> Response {
    let client = match crate::runtime_control_api::client(&state) {
        Ok(client) => client,
        Err(error) => return crate::runtime_control_api::runtime_error(&error),
    };
    match client.proxies().await {
        Ok(proxies) => Json(leaf_nodes(proxies)).into_response(),
        Err(error) => crate::runtime_control_api::runtime_error(&error),
    }
}

async fn debug_node(State(state): State<Arc<AppState>>, Json(input): Json<DebugInput>) -> Response {
    let name = input.name.trim();
    if name.is_empty() || name.len() > 512 {
        return api_error(
            StatusCode::BAD_REQUEST,
            "INVALID_NODE",
            "node name is required and must not exceed 512 bytes",
        );
    }
    let client = match crate::runtime_control_api::client(&state) {
        Ok(client) => client,
        Err(error) => return crate::runtime_control_api::runtime_error(&error),
    };
    let proxies = match client.proxies().await {
        Ok(proxies) => proxies,
        Err(error) => return crate::runtime_control_api::runtime_error(&error),
    };
    if !leaf_nodes(proxies).iter().any(|node| node.name == name) {
        return api_error(
            StatusCode::NOT_FOUND,
            "NODE_NOT_FOUND",
            "proxy node not found",
        );
    }

    let (sender, receiver) = mpsc::channel(32);
    tokio::spawn(run_debug(state, name.to_owned(), sender));
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
        let item = receiver.recv().await?;
        let event = Event::default().event(item.event).json_data(item.payload);
        Some((event, receiver))
    });
    Sse::new(stream).into_response()
}

fn leaf_nodes(proxies: Vec<sempre_core_control::Proxy>) -> Vec<Node> {
    let mut seen = HashSet::new();
    let mut nodes = proxies
        .into_iter()
        .filter(|proxy| proxy.all.is_empty() && is_leaf_type(&proxy.proxy_type))
        .filter(|proxy| seen.insert(proxy.name.clone()))
        .map(|proxy| Node {
            name: proxy.name,
            node_type: proxy.proxy_type,
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|left, right| left.name.cmp(&right.name));
    nodes
}

fn is_leaf_type(proxy_type: &str) -> bool {
    !matches!(
        proxy_type.to_ascii_lowercase().as_str(),
        "selector"
            | "urltest"
            | "url-test"
            | "fallback"
            | "loadbalance"
            | "load-balance"
            | "direct"
            | "reject"
            | "rejectdrop"
            | "pass"
            | "dns"
            | "compatible"
    )
}

async fn run_debug(state: Arc<AppState>, node: String, sender: mpsc::Sender<DebugEvent>) {
    let total = Instant::now();
    if !send_running(&sender, "prepare", "启动隔离 Core").await {
        return;
    }
    let prepared = Instant::now();
    let core = match DiagnosticCore::start(&state.manager, &node).await {
        Ok(core) => core,
        Err(error) => {
            send_failed(&sender, "prepare", "启动隔离 Core", prepared, &error).await;
            send_error(&sender, error).await;
            return;
        }
    };
    if !send_succeeded(
        &sender,
        "prepare",
        "启动隔离 Core",
        prepared,
        json!({"node":node}),
    )
    .await
    {
        core.stop().await;
        return;
    }

    run_http_step(
        &sender,
        &core,
        "probe",
        "节点探活",
        "https://cp.cloudflare.com/generate_204",
    )
    .await;
    run_dns_step(
        &sender,
        &core,
        "dns-baidu",
        "DNS · www.baidu.com",
        "www.baidu.com",
    )
    .await;
    run_dns_step(
        &sender,
        &core,
        "dns-google",
        "DNS · www.google.com",
        "www.google.com",
    )
    .await;
    run_http_step(
        &sender,
        &core,
        "http-baidu",
        "HTTP · www.baidu.com",
        "https://www.baidu.com/",
    )
    .await;
    run_http_step(
        &sender,
        &core,
        "http-google",
        "HTTP · www.google.com",
        "https://www.google.com/generate_204",
    )
    .await;
    core.stop().await;
    let _ = sender
        .send(DebugEvent {
            event: "done",
            payload: json!({"node":node,"duration_ms":millis(total)}),
        })
        .await;
}

async fn run_http_step(
    sender: &mpsc::Sender<DebugEvent>,
    core: &DiagnosticCore,
    id: &'static str,
    label: &'static str,
    url: &'static str,
) {
    if !send_running(sender, id, label).await {
        return;
    }
    let started = Instant::now();
    match core.http_get(url).await {
        Ok(result) => {
            let data = serde_json::to_value(result).unwrap_or(Value::Null);
            send_succeeded(sender, id, label, started, data).await;
        }
        Err(error) => send_failed(sender, id, label, started, &error).await,
    }
}

async fn run_dns_step(
    sender: &mpsc::Sender<DebugEvent>,
    core: &DiagnosticCore,
    id: &'static str,
    label: &'static str,
    domain: &'static str,
) {
    if !send_running(sender, id, label).await {
        return;
    }
    let started = Instant::now();
    match core.dns_query(domain).await {
        Ok(result) => {
            let data = serde_json::to_value(result).unwrap_or(Value::Null);
            send_succeeded(sender, id, label, started, data).await;
        }
        Err(error) => send_failed(sender, id, label, started, &error).await,
    }
}

async fn send_running(
    sender: &mpsc::Sender<DebugEvent>,
    id: &'static str,
    label: &'static str,
) -> bool {
    sender
        .send(DebugEvent {
            event: "step",
            payload: json!({"id":id,"label":label,"state":"running"}),
        })
        .await
        .is_ok()
}

async fn send_succeeded(
    sender: &mpsc::Sender<DebugEvent>,
    id: &'static str,
    label: &'static str,
    started: Instant,
    data: Value,
) -> bool {
    sender
        .send(DebugEvent {
            event: "step",
            payload: json!({"id":id,"label":label,"state":"succeeded","duration_ms":millis(started),"data":data}),
        })
        .await
        .is_ok()
}

async fn send_failed(
    sender: &mpsc::Sender<DebugEvent>,
    id: &'static str,
    label: &'static str,
    started: Instant,
    error: &str,
) {
    let _ = sender
        .send(DebugEvent {
            event: "step",
            payload: json!({"id":id,"label":label,"state":"failed","duration_ms":millis(started),"error":error}),
        })
        .await;
}

async fn send_error(sender: &mpsc::Sender<DebugEvent>, message: String) {
    let _ = sender
        .send(DebugEvent {
            event: "error",
            payload: json!({"message":message}),
        })
        .await;
}

fn millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_groups_and_builtin_actions() {
        let proxies = vec![
            proxy("group", "Selector", &["node-a"]),
            proxy("DIRECT", "Direct", &[]),
            proxy("node-a", "Shadowsocks", &[]),
            proxy("node-a", "Shadowsocks", &[]),
            proxy("node-b", "Vless", &[]),
        ];
        assert_eq!(
            leaf_nodes(proxies),
            vec![
                Node {
                    name: "node-a".into(),
                    node_type: "Shadowsocks".into()
                },
                Node {
                    name: "node-b".into(),
                    node_type: "Vless".into()
                },
            ]
        );
    }

    fn proxy(name: &str, proxy_type: &str, all: &[&str]) -> sempre_core_control::Proxy {
        sempre_core_control::Proxy {
            name: name.into(),
            proxy_type: proxy_type.into(),
            all: all.iter().map(|value| (*value).into()).collect(),
            ..Default::default()
        }
    }
}
