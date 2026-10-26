use super::{
    Event,
    types::{Message, ProbeResult},
};
use crate::leak_detection_api::{
    correlate,
    model::Input,
    platform::{PlatformSnapshot, SocketInfo},
};
use std::{
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::{mpsc, oneshot},
};

#[derive(Clone)]
pub(super) struct Context {
    pub input: Input,
    pub core: Option<sempre_core_control::Client>,
    pub proxies: Vec<sempre_core_control::Proxy>,
    pub ranges: Vec<ipnet::IpNet>,
}

pub(super) async fn run(id: String, context: Context, sender: mpsc::Sender<Event>) {
    let started = Instant::now();
    let outcome =
        tokio::time::timeout(Duration::from_secs(18), execute(&id, &context, &sender)).await;
    let mut result = match outcome {
        Ok(Ok(result)) => result,
        Ok(Err(detail)) => result(&id, "failed", &detail),
        Err(_) => result(
            &id,
            "timeout",
            "该项探针超过 18 秒，子进程已终止；其余项目独立继续",
        ),
    };
    result.elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let _ = sender.send(Event::Result(result)).await;
}
fn result(id: &str, status: &str, detail: &str) -> ProbeResult {
    ProbeResult {
        id: id.into(),
        status: status.into(),
        detail: detail.into(),
        pid: None,
        addresses: vec![],
        public_ip: None,
        elapsed_ms: 0,
    }
}
async fn execute(
    id: &str,
    context: &Context,
    sender: &mpsc::Sender<Event>,
) -> Result<ProbeResult, String> {
    let executable = std::env::current_exe().map_err(|_| "无法定位原生探针程序")?;
    let mut child = Command::new(executable)
        .args(["leak-probe", id])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "无法启动独立探针进程")?;
    let pid = child.id().ok_or("探针进程标识不可用")?;
    let mut input = child.stdin.take().ok_or("探针控制通道不可用")?;
    let stdout = child.stdout.take().ok_or("探针输出不可用")?;
    let mut lines = BufReader::new(stdout.take(16_384)).lines();
    let first = lines
        .next_line()
        .await
        .map_err(|_| "探针握手失败")?
        .ok_or("探针提前退出")?;
    if !matches!(serde_json::from_str::<Message>(&first), Ok(Message::Ready)) {
        return Err("探针未返回就绪握手".into());
    }
    let process =
        crate::leak_detection_api::platform::probe_process(pid).ok_or("探针进程身份无法确认")?;
    let (activate, activated) = oneshot::channel();
    sender
        .send(Event::Registered {
            id: id.into(),
            process: process.clone(),
            activate,
        })
        .await
        .map_err(|_| "观测已结束")?;
    activated.await.map_err(|_| "探针在开始前被取消")?;
    input
        .write_all(b"start\n")
        .await
        .map_err(|_| "探针启动失败")?;
    while let Some(line) = lines.next_line().await.map_err(|_| "探针输出读取失败")? {
        match serde_json::from_str::<Message>(&line).map_err(|_| "探针消息格式无效")? {
            Message::Connected {
                local,
                remote,
                phase,
            } => {
                observe_socket(&process, local, remote, &phase, context, sender).await?;
                input
                    .write_all(b"continue\n")
                    .await
                    .map_err(|_| "探针连接已停止")?;
            }
            Message::Result {
                addresses,
                public_ip,
                detail,
            } => {
                let mut result = result(id, "passed", &detail);
                result.pid = Some(pid);
                result.addresses = addresses;
                result.public_ip = public_ip;
                let _ = child.wait().await;
                return Ok(result);
            }
            Message::Error { detail } => return Err(detail),
            Message::Ready => return Err("重复的探针握手".into()),
        }
    }
    Err("探针退出且未返回终态；结果不完整".into())
}

async fn observe_socket(
    process: &crate::leak_detection_api::platform::ProcessInfo,
    local: std::net::SocketAddr,
    remote: std::net::SocketAddr,
    phase: &str,
    context: &Context,
    sender: &mpsc::Sender<Event>,
) -> Result<(), String> {
    let snapshot = PlatformSnapshot {
        platform: std::env::consts::OS.into(),
        processes: vec![process.clone()],
        sockets: vec![SocketInfo {
            pid: process.pid,
            protocol: "tcp".into(),
            local_ip: local.ip().to_string(),
            local_port: local.port(),
            remote_ip: remote.ip().to_string(),
            remote_port: remote.port(),
        }],
        dns: vec![],
        gaps: vec![],
    };
    let core = if let Some(core) = &context.core {
        tokio::time::timeout(Duration::from_secs(2), core.connections())
            .await
            .ok()
            .and_then(Result::ok)
    } else {
        None
    };
    let mut records = correlate::connections(
        &context.input,
        &snapshot,
        core.as_ref(),
        &context.proxies,
        &context.ranges,
    );
    for record in &mut records {
        record.evidence.push(format!("真实探针进程在 {phase} 阶段报告同一连接源端口；连接保持期间读取内核，未把合成流量归属 Codex/Claude Code"));
    }
    sender
        .send(Event::Records(records))
        .await
        .map_err(|_| "观测已结束")?;
    Ok(())
}
