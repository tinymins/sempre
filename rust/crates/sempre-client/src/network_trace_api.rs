mod capture;

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::post,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::api::AppState;

#[derive(Deserialize)]
struct Input {
    target: String,
}

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/network/diagnostics/preview", post(preview))
        .route("/api/v1/network/diagnostics/trace", post(trace))
}

#[derive(Clone)]
struct Session {
    id: String,
    started: Instant,
    sender: mpsc::UnboundedSender<(&'static str, Value)>,
}

impl Session {
    fn send(&self, name: &'static str, data: Value) {
        let _ = self.sender.send((name, data));
    }

    fn step(
        &self,
        id: &str,
        phase: &str,
        status: &str,
        source: &str,
        detail: &str,
        evidence: Vec<String>,
    ) {
        let evidence = Value::from(evidence);
        self.send(
            "trace-step",
            json!({ "trace_id": self.id, "step": {
                "id": id, "phase": phase, "title": title(phase), "status": status,
                "source": source, "detail": detail, "evidence": evidence,
                "elapsed_ms": self.started.elapsed().as_millis(),
                "attempt_id": if id.starts_with("attempt-1") { Some("attempt-1") } else { None },
            }}),
        );
    }
}

fn title(phase: &str) -> &'static str {
    match phase {
        "frontend" => "Sempre 前置 DNS",
        "dns" => "地址解析",
        "connection" => "实际连接",
        "core" => "内核连接证据",
        "tls" => "TLS 握手",
        "http" => "HTTP 响应",
        _ => "诊断",
    }
}

async fn preview(State(state): State<Arc<AppState>>, Json(input): Json<Input>) -> Response {
    let (_, host, _) = match sempre_network::parse_trace_target(&input.target) {
        Ok(target) => target,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"message": error.to_string()})),
            )
                .into_response();
        }
    };
    let evidence = if host.trim_matches(['[', ']']).parse::<IpAddr>().is_ok() {
        vec!["目标为 IP 地址，无需 DNS 查询；业务连接出口待实测".into()]
    } else if let Some(frontend) = state.manager.dns_diagnostic_handle().await {
        frontend.preview(&host)
    } else {
        vec!["当前无运行中的前置解析器；系统解析及实际出站待实测".into()]
    };
    let detail = evidence
        .last()
        .cloned()
        .unwrap_or_else(|| "等待测试".into());
    Json(json!({"mode":"preview", "target":input.target, "steps":[{
        "id":"preview-frontend", "phase":"frontend", "title":"规则预览 · 尚未测试",
        "status":"unknown", "source":"config", "detail":detail,
        "evidence":evidence, "elapsed_ms":0
    }]}))
    .into_response()
}

async fn trace(State(state): State<Arc<AppState>>, Json(input): Json<Input>) -> Response {
    let target = match sempre_network::parse_trace_target(&input.target) {
        Ok(target) => target,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"message":error.to_string()})),
            )
                .into_response();
        }
    };
    let (sender, receiver) = mpsc::unbounded_channel();
    let session = Session {
        id: uuid::Uuid::new_v4().to_string(),
        started: Instant::now(),
        sender,
    };
    tokio::spawn(async move {
        session.send(
            "trace-started",
            json!({"trace_id":session.id,"target":input.target}),
        );
        let work = run(&state, &session, target);
        tokio::select! {
            () = session.sender.closed() => {},
            result = tokio::time::timeout(Duration::from_secs(35), work) => {
                let status = if let Ok(status) = result { status } else {
                        session.step("deadline", "connection", "failed", "session", "本次测试超过 35 秒，已停止采集和请求", vec![]);
                        "failed"
                };
                session.send("trace-completed", json!({"trace_id":session.id,"status":status}));
            }
        }
    });
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
        let (name, data) = receiver.recv().await?;
        Some((Event::default().event(name).json_data(data), receiver))
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

async fn run(
    state: &AppState,
    session: &Session,
    (url, host, port): (url::Url, String, u16),
) -> &'static str {
    let host = host.trim_matches(['[', ']']);
    session.step(
        "dns",
        "dns",
        "running",
        "session",
        "开始本次请求的地址解析",
        vec![],
    );
    let mut frontend_resolution = false;
    let addresses = if let Ok(address) = host.parse::<IpAddr>() {
        session.step(
            "dns",
            "dns",
            "passed",
            "socket",
            "目标为 IP 地址，无需 DNS",
            vec![address.to_string()],
        );
        vec![address]
    } else if let Some(addresses) = frontend_addresses(state, session, host).await {
        frontend_resolution = true;
        addresses
    } else {
        session.step(
            "frontend",
            "frontend",
            "unknown",
            "session",
            "无运行中的前置解析器；本次使用系统解析",
            vec![],
        );
        match tokio::time::timeout(
            Duration::from_secs(5),
            tokio::net::lookup_host((host, port)),
        )
        .await
        {
            Ok(Ok(result)) => {
                let addresses = result.map(|address| address.ip()).collect::<Vec<_>>();
                session.step(
                    "dns",
                    "dns",
                    "passed",
                    "socket",
                    "系统解析实际返回；未观测解析器内部路径",
                    addresses.iter().map(ToString::to_string).collect(),
                );
                addresses
            }
            result => {
                let detail = match result {
                    Ok(Err(error)) => error.to_string(),
                    _ => "System DNS lookup timed out".into(),
                };
                session.step("dns", "dns", "failed", "socket", &detail, vec![]);
                return "failed";
            }
        }
    };
    let Some(address) = addresses.first() else {
        return "failed";
    };
    session.step(
        "attempt-1-connection",
        "connection",
        "running",
        "socket",
        "对首个解析地址发起一次连接；不自动重试或跟随重定向",
        vec![format!("destination={address}:{port}")],
    );
    capture::run(
        state,
        session,
        &url,
        SocketAddr::new(*address, port),
        frontend_resolution.then_some(host),
    )
    .await
}

async fn frontend_addresses(
    state: &AppState,
    session: &Session,
    host: &str,
) -> Option<Vec<IpAddr>> {
    let frontend = state.manager.dns_diagnostic_handle().await?;
    session.step(
        "frontend",
        "frontend",
        "running",
        "frontend",
        "调用正在运行的前置解析器；不验证系统 DNS 接管",
        vec![],
    );
    let mut addresses = Vec::new();
    let mut evidence = vec![
        "本次请求直接调用运行中的解析器，随后连接其返回地址；不代表其他应用的解析路径。".into(),
    ];
    for record_type in ["A", "AAAA"] {
        let query = tokio::time::timeout(
            Duration::from_secs(5),
            frontend.resolve_with_progress(host, record_type, |step| {
                evidence.push(format!("{record_type}: {step}"));
                session.step(
                    "frontend",
                    "frontend",
                    "running",
                    "frontend",
                    "前置规则实际判断中",
                    evidence.clone(),
                );
            }),
        )
        .await;
        match query {
            Ok(Ok(result)) => {
                evidence.push(format!(
                    "{record_type}: upstream={} rcode={} decision={}",
                    result.upstream, result.response_code, result.detail
                ));
                evidence.extend(
                    result
                        .addresses
                        .iter()
                        .map(|ip| format!("{record_type}: {ip}")),
                );
                if result.response_code == 0 {
                    addresses.extend(result.addresses);
                }
            }
            Ok(Err(error)) => evidence.push(format!("{record_type}: {error}")),
            Err(_) => evidence.push(format!(
                "{record_type}: DNS query timed out after 5 seconds"
            )),
        }
        session.step(
            "frontend",
            "frontend",
            if record_type == "A" {
                "running"
            } else if addresses.is_empty() {
                "failed"
            } else {
                "passed"
            },
            "frontend",
            "前置 DNS 实际执行记录；DNS 上游选择不等于业务出站",
            evidence.clone(),
        );
    }
    session.step(
        "dns",
        "dns",
        if addresses.is_empty() {
            "failed"
        } else {
            "passed"
        },
        "frontend",
        "使用本次前置 DNS 返回的地址",
        addresses.iter().map(ToString::to_string).collect(),
    );
    Some(addresses)
}
