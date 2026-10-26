mod capture_evidence;
mod correlate;
mod model;
mod observe;
mod platform;
mod probes;
pub(crate) use probes::helper_main as probe_helper_main;

use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use serde_json::{Value, json};
use tokio::sync::{Semaphore, mpsc};

use crate::api::AppState;
use model::Input;

static OBSERVATION: Semaphore = Semaphore::const_new(1);

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/network/leak-detection/preflight", get(preflight))
        .route("/api/v1/network/leak-detection/observe", post(observe))
        .route(
            "/api/v1/network/leak-detection/observe/{id}/probes",
            post(probes::control::start),
        )
        .route(
            "/api/v1/network/leak-detection/observe/{id}/probes/stop",
            post(probes::control::stop),
        )
}

async fn preflight(State(state): State<Arc<AppState>>) -> Response {
    let Ok(platform) =
        tokio::time::timeout(Duration::from_secs(10), platform::collect("all")).await
    else {
        return error(StatusCode::REQUEST_TIMEOUT, "进程与套接字检查超时");
    };
    let core_available = crate::runtime_control_api::client(&state).is_ok();
    Json(json!({
        "platform": platform.platform,
        "processes": platform.processes,
        "capabilities": [
            {"id":"processes", "status":"partial", "detail":"采样当前可见的应用进程与子进程；独立启动的 MCP 不自动归属"},
            {"id":"core", "status":if core_available {"partial"} else {"unavailable"}, "detail":"内核控制配置可读性检查；实际连通性将在检测时验证，短连接可能漏采"},
            {"id":"capture", "status":"partial", "detail":"开始后尝试仅采集接口包头；权限不足或系统不支持会显示覆盖缺口"},
            {"id":"dns", "status":"partial", "detail":"DNS 套接字与被动接口证据；共享系统解析器只能标为设备级，DoH 内容不可见"},
            {"id":"gateway", "status":"partial", "detail":"按客户端 IP 观察网关内核；客户端进程、绕过此网关及 NAT 后关联需要额外客户端证据"}
        ],
        "gaps": platform.gaps,
    })).into_response()
}

fn error(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(json!({"error": {"code": "LEAK_DETECTION", "message": message}})),
    )
        .into_response()
}

async fn observe(State(state): State<Arc<AppState>>, Json(mut input): Json<Input>) -> Response {
    if !(10..=120).contains(&input.duration_seconds)
        || !["codex", "claude", "all"].contains(&input.target.as_str())
        || !["local", "gateway"].contains(&input.topology.as_str())
    {
        return error(
            StatusCode::BAD_REQUEST,
            "检测时长须为 10–120 秒，且需选择支持的应用和部署方式",
        );
    }
    if input.topology == "gateway" {
        let Some(ip) = input
            .client_ip
            .as_ref()
            .and_then(|ip| ip.parse::<std::net::IpAddr>().ok())
        else {
            return error(
                StatusCode::BAD_REQUEST,
                "网关模式需要客户端的 IPv4 或 IPv6 地址",
            );
        };
        if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
            return error(StatusCode::BAD_REQUEST, "请输入实际客户端地址");
        }
        input.client_ip = Some(ip.to_string());
    }
    let Ok(permit) = OBSERVATION.try_acquire() else {
        return error(StatusCode::CONFLICT, "已有检测正在进行，请先停止或等待完成");
    };
    let (sender, receiver) = mpsc::channel::<(&'static str, Value)>(4);
    let id = uuid::Uuid::new_v4().to_string();
    let (commands, receiver_commands) = mpsc::channel(2);
    let guard = probes::control::register(
        &id,
        input.topology == "local",
        input.duration_seconds,
        commands,
    );
    tokio::spawn(async move {
        let _permit = permit;
        let _guard = guard;
        tokio::select! {
            () = sender.closed() => {},
            _ = tokio::time::timeout(Duration::from_secs(input.duration_seconds + 10), observe::run(&state, input, &sender, id, receiver_commands)) => {},
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
