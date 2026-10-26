use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::{
    capture_evidence, correlate,
    model::{Input, Report, Summary},
    platform, probes,
};
use crate::api::AppState;

pub(super) async fn run(
    state: &AppState,
    input: Input,
    sender: &mpsc::Sender<(&'static str, Value)>,
    id: String,
    mut commands: mpsc::Receiver<probes::control::Command>,
) {
    let started = Instant::now();
    let mut report = initial_report(&input, id);
    if sender.send(("snapshot", json!(report))).await.is_err() {
        return;
    }
    if sender
        .send((
            "status",
            json!({"message":"正在准备采集器；就绪后可发起主动探针，也可观察真实应用流量"}),
        ))
        .await
        .is_err()
    {
        return;
    }
    let mut capture = platform::start_capture().await;
    let client = crate::runtime_control_api::client(state).ok();
    let ranges = state.manager.runtime_fakeip_ranges().unwrap_or_else(|_| {
        report.gap("无法读取运行时 FakeIP 范围；不进行 FakeIP 地址转换关联");
        vec![]
    });
    let proxies = if let Some(client) = &client {
        if let Ok(Ok(proxies)) =
            tokio::time::timeout(Duration::from_secs(4), client.proxies()).await
        {
            proxies
        } else {
            report.gap("无法读取内核出站类型；命中的路由链仍列出，但不据名称判断代理或直连");
            vec![]
        }
    } else {
        report.gap("当前没有可读取的内核控制接口；仅采集进程套接字，无法判断是否进入 Sempre");
        vec![]
    };
    let mut probes = probes::Session::new(
        input.clone(),
        client.clone(),
        proxies.clone(),
        ranges.clone(),
    );
    while started.elapsed() < Duration::from_secs(input.duration_seconds) {
        let remaining =
            Duration::from_secs(input.duration_seconds).saturating_sub(started.elapsed());
        let processes = probes.processes.clone();
        let collection = tokio::time::timeout(
            remaining.min(Duration::from_secs(8)),
            sample(&input, client.as_ref(), &processes),
        );
        let Ok((platform, core)) =
            drive(collection, &mut probes, &mut report, &mut commands, sender).await
        else {
            report.gap("进程或连接采样超时；本轮覆盖不完整");
            break;
        };
        report.ready = true;
        probes::control::ready(&report.id);
        if report.topology == "local" && report.probes.status == "idle" {
            report.probes.available = started.elapsed().as_secs() + 20 <= input.duration_seconds;
            report.probes.reason = if report.probes.available {
                "可发起独立探针；结果与真实应用分开"
            } else {
                "剩余观测时间不足 20 秒；请延长检测时间后重试"
            }
            .into();
        }
        probes.sampled(&platform.processes, &mut report);
        if client.is_some() && core.is_none() {
            report.gap("内核连接采样失败或超时；该时段无法关联路由证据");
        }
        merge_platform(&mut report, &platform, &input);
        let records = correlate::connections(&input, &platform, core.as_ref(), &proxies, &ranges);
        let current_ids: Vec<_> = records.iter().map(|record| record.id.clone()).collect();
        merge_records(&mut report, records);
        capture_evidence::merge(&mut report, capture.snapshot(), &ranges, &current_ids);
        report.elapsed_seconds = started.elapsed().as_secs();
        report.summarize();
        if sender.send(("snapshot", json!(report))).await.is_err() {
            return;
        }
        let remaining =
            Duration::from_secs(input.duration_seconds).saturating_sub(started.elapsed());
        drive(
            tokio::time::sleep(remaining.min(Duration::from_secs(1))),
            &mut probes,
            &mut report,
            &mut commands,
            sender,
        )
        .await;
    }
    capture_evidence::merge(&mut report, capture.snapshot(), &ranges, &[]);
    drop(capture);
    probes.stop(&mut report);
    report.ready = false;
    finish(&mut report, started);
    let _ = sender.send(("result", json!(report))).await;
}

fn initial_report(input: &Input, id: String) -> Report {
    let mut report = Report {
        id,
        started_at: chrono::Utc::now().to_rfc3339(),
        elapsed_seconds: 0,
        target: input.target.clone(),
        topology: input.topology.clone(),
        platform: std::env::consts::OS.into(),
        processes: vec![],
        connections: vec![],
        dns: vec![],
        gaps: vec![],
        summary: Summary::default(),
        completed: false,
        ready: false,
        probes: probes::types::ProbeState::new(input.topology == "local"),
        probe_summary: Summary::default(),
        capture: platform::CaptureSnapshot {
            status: "pending".into(),
            detail: "等待接口采集".into(),
            packets: vec![],
        },
    };
    report.gap("仅覆盖本次观测窗口；未触发的连接、短连接、后续配置变化和独立 MCP 不在已验证范围");
    report.gap("内核选择代理不证明物理出口；未关联到内核也不证明绕过 Sempre");
    report.gap("进程、套接字、内核与包头分时采样；关联不是进程级全量抓包，存在时序与端口复用盲区");
    report
        .gap("共享系统 DNS 无法可靠归属应用；DoH/DoT 的查询内容不可见，缺少 DNS 证据不等于无泄漏");
    if input.topology == "gateway" {
        report.gap(
            "网关模式仅按指定客户端 IP 观察内核连接；无法识别客户端应用或发现完全绕过此网关的流量",
        );
        report.gap("客户端可能同时使用 IPv4 与 IPv6；本次只覆盖输入的一个源地址，NAT 后流量无法仅凭此地址归属");
    }
    report
}

fn merge_platform(report: &mut Report, platform: &platform::PlatformSnapshot, input: &Input) {
    report.platform.clone_from(&platform.platform);
    for gap in &platform.gaps {
        report.gap(gap.clone());
    }
    if input.topology == "local" {
        for process in &platform.processes {
            if !report.processes.iter().any(|existing| {
                existing.pid == process.pid && existing.started_at == process.started_at
            }) && report.processes.len() < 256
            {
                report.processes.push(process.clone());
            }
        }
        for dns in &platform.dns {
            if !report.dns.iter().any(|existing| {
                existing.scope == dns.scope
                    && existing.protocol == dns.protocol
                    && existing.server == dns.server
            }) && report.dns.len() < 200
            {
                report.dns.push(dns.clone());
            }
        }
    }
}

pub(super) fn merge_records(report: &mut Report, records: Vec<super::model::Connection>) {
    for record in records {
        let index = report
            .connections
            .iter()
            .position(|existing| existing.id == record.id)
            .or_else(|| {
                if record.chains.is_empty() {
                    None
                } else {
                    report.connections.iter().position(|existing| {
                        existing.socket_key == record.socket_key && existing.chains.is_empty()
                    })
                }
            });
        if let Some(index) = index {
            let existing = &mut report.connections[index];
            let physical_egress = existing.physical_egress;
            *existing = record;
            if physical_egress {
                existing.physical_egress = true;
                existing.evidence.push(
                    "本次观测窗口曾匹配物理接口包头；详见接口记录，显式代理端点仍需核对".into(),
                );
            }
        } else if report.connections.len() < 400 {
            // Different observed routes stay separate, including an earlier DIRECT route.
            report.connections.push(record);
        } else {
            report.gap("连接记录达到 400 条上限；额外连接未保留");
        }
    }
}

async fn sample(
    input: &Input,
    client: Option<&sempre_core_control::Client>,
    processes: &[platform::ProcessInfo],
) -> (
    platform::PlatformSnapshot,
    Option<sempre_core_control::ConnectionSnapshot>,
) {
    let (platform, core) = tokio::join!(
        platform::collect_with_probes(
            if input.topology == "gateway" {
                "none"
            } else {
                &input.target
            },
            processes
        ),
        async {
            if let Some(client) = client {
                tokio::time::timeout(Duration::from_secs(3), client.connections())
                    .await
                    .ok()
                    .and_then(Result::ok)
            } else {
                None
            }
        }
    );
    (platform, core)
}

async fn drive<F: std::future::Future>(
    future: F,
    probes: &mut probes::Session,
    report: &mut Report,
    commands: &mut mpsc::Receiver<probes::control::Command>,
    sender: &mpsc::Sender<(&'static str, Value)>,
) -> F::Output {
    tokio::pin!(future);
    loop {
        tokio::select! {
            result = &mut future => return result,
            Some(command) = commands.recv() => {
                probes.command(command, report);
                let _ = sender.send(("probe", json!(report.probes))).await;
            },
            Some(event) = probes.receiver.recv() => {
                probes.apply(event, report);
                report.summarize();
                let _ = sender.send(("probe", json!(report.probes))).await;
            },
        }
    }
}

fn finish(report: &mut Report, started: Instant) {
    report.summarize();
    if !report
        .connections
        .iter()
        .any(|connection| connection.app != "probe")
    {
        report.gap("未采集到可归属的应用连接；请确认进程、客户端地址与权限，并触发实际请求后重试");
    }
    if report.summary.ipv4 == 0 {
        report.gap("未覆盖可识别的 IPv4 应用连接");
    }
    if report.summary.ipv6 == 0 {
        report.gap("未覆盖可识别的 IPv6 应用连接；不能据此认定 IPv6 已禁用或无泄漏");
    }
    report.elapsed_seconds = started.elapsed().as_secs();
    report.completed = true;
}
