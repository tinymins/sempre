use std::{collections::HashSet, net::SocketAddr, time::Duration};

use sempre_core_control::{Connection, ConnectionSnapshot};
use sempre_network::TraceTransportEvent;
use tokio::{sync::mpsc, time::timeout};

use super::{AppState, Session};

pub(super) async fn run(
    state: &AppState,
    session: &Session,
    url: &url::Url,
    remote: SocketAddr,
    observed_host: Option<&str>,
) -> &'static str {
    session.step(
        "attempt-1-core",
        "core",
        "running",
        "session",
        "准备内核连接采集；尚未证实经过内核",
        vec![],
    );
    // Establish and consume the first snapshot before creating the diagnostic socket.
    let preparation = async {
        let client =
            crate::runtime_control_api::client(state).map_err(|error| error.to_string())?;
        let mut stream = client
            .connection_trace_stream()
            .await
            .map_err(|error| error.to_string())?;
        let snapshot = stream
            .next_connections()
            .await
            .map_err(|error| error.to_string())?;
        Ok::<_, String>((stream, snapshot))
    };
    let (mut stream, mut snapshot, old_ids) =
        match timeout(Duration::from_secs(2), preparation).await {
            Ok(Ok((stream, snapshot))) => {
                let ids = snapshot
                    .connections
                    .iter()
                    .map(|connection| connection.id.clone())
                    .collect::<HashSet<_>>();
                (Some(stream), snapshot, ids)
            }
            result => {
                let reason = match result {
                    Ok(Err(error)) => error,
                    _ => "Core snapshot preparation timed out".into(),
                };
                session.step(
                    "attempt-1-core",
                    "core",
                    "unknown",
                    "session",
                    "内核采集不可用，仍继续真实请求",
                    vec![reason],
                );
                (None, ConnectionSnapshot::default(), HashSet::new())
            }
        };
    let (sender, mut events) = mpsc::unbounded_channel();
    let work = sempre_network::trace_transport(url, remote, move |event| {
        let _ = sender.send(event);
    });
    tokio::pin!(work);
    let mut local = None;
    let mut observed = None;
    let mut phase = "connection";
    let outcome = loop {
        tokio::select! {
            biased;
            Some(event) = events.recv() => {
                transport_event(session, event, &mut local, &mut phase);
                inspect(session, &snapshot, &old_ids, local, remote, observed_host, &mut observed);
            }
            result = async { stream.as_mut().expect("guarded stream").next_connections().await }, if stream.is_some() => {
                match result {
                    Ok(next) => {
                        snapshot = next;
                        inspect(session, &snapshot, &old_ids, local, remote, observed_host, &mut observed);
                    }
                    Err(error) => {
                        stream = None;
                        if observed.is_none() {
                            session.step("attempt-1-core", "core", "unknown", "session", "内核采集已中断，不能据此推断直连", vec![error.to_string()]);
                        }
                    }
                }
            }
            result = &mut work => break result,
        }
    };
    while let Ok(event) = events.try_recv() {
        transport_event(session, event, &mut local, &mut phase);
        inspect(
            session,
            &snapshot,
            &old_ids,
            local,
            remote,
            observed_host,
            &mut observed,
        );
    }
    finish(session, outcome, observed.is_some(), phase)
}

fn transport_event(
    session: &Session,
    event: TraceTransportEvent,
    local: &mut Option<SocketAddr>,
    phase: &mut &'static str,
) {
    match event {
        TraceTransportEvent::Connected {
            local: endpoint,
            remote,
        } => {
            *local = Some(endpoint);
            session.step(
                "attempt-1-connection",
                "connection",
                "passed",
                "socket",
                "TCP 已连接；本地 socket 信息不证明实际出口",
                vec![
                    format!("source={endpoint}"),
                    format!("destination={remote}"),
                ],
            );
        }
        TraceTransportEvent::Stage {
            phase: next,
            complete,
            detail,
        } => {
            *phase = next;
            session.step(
                &format!("attempt-1-{next}"),
                next,
                if complete { "passed" } else { "running" },
                if next == "http" { "http" } else { "socket" },
                &detail,
                vec!["同一次 TCP 连接；未跟随 HTTP 重定向。".into()],
            );
        }
    }
}

fn inspect(
    session: &Session,
    snapshot: &ConnectionSnapshot,
    old_ids: &HashSet<String>,
    local: Option<SocketAddr>,
    remote: SocketAddr,
    observed_host: Option<&str>,
    observed: &mut Option<String>,
) {
    let Some(local) = local else {
        return;
    };
    let matches = snapshot
        .connections
        .iter()
        .filter(|connection| {
            !connection.id.is_empty()
                && !old_ids.contains(&connection.id)
                && matches_socket(connection, local, remote, observed_host)
        })
        .collect::<Vec<_>>();
    let [connection] = matches.as_slice() else {
        return;
    };
    if observed.as_ref().is_some_and(|id| id != &connection.id) {
        return;
    }
    *observed = Some(connection.id.clone());
    let mut evidence = vec![
        format!(
            "correlation={}",
            if connection
                .metadata
                .destination_ip
                .parse::<std::net::IpAddr>()
                .ok()
                == Some(remote.ip())
            {
                "exact socket tuple"
            } else {
                "exact source endpoint + TCP + destination port + core fake-ip mode + observed frontend answer-to-host mapping"
            }
        ),
        format!("connection_id={}", connection.id),
        format!("source={local}; destination={remote}; protocol=TCP"),
        format!(
            "rule={}; payload={}",
            connection.rule, connection.rule_payload
        ),
        format!("chains (core API order)={}", connection.chains.join(" → ")),
        "内核原始策略组链，不是多条规则依次命中；出站选择不等于请求成功。".into(),
    ];
    if let Ok(raw) = serde_json::to_string(connection) {
        evidence.push(raw);
    }
    session.send("trace-step", serde_json::json!({"trace_id":session.id,"step":{
        "id":"attempt-1-core", "phase":"core", "title":"内核连接证据",
        "status":"passed", "source":"core", "detail":"已通过本次 socket 端点关联内核连接",
        "evidence":evidence, "elapsed_ms":session.started.elapsed().as_millis(),
        "attempt_id":"attempt-1", "rule":{"type":connection.rule,"payload":connection.rule_payload},
        "chains":connection.chains, "chain_order":"core_api"
    }}));
}

fn matches_socket(
    connection: &Connection,
    local: SocketAddr,
    remote: SocketAddr,
    observed_host: Option<&str>,
) -> bool {
    let metadata = &connection.metadata;
    metadata.network.eq_ignore_ascii_case("tcp")
        && metadata.source_ip.parse::<std::net::IpAddr>().ok() == Some(local.ip())
        && metadata.source_port.parse::<u16>().ok() == Some(local.port())
        && (metadata.destination_ip.parse::<std::net::IpAddr>().ok() == Some(remote.ip())
            || (metadata.dns_mode == "fake-ip"
                && observed_host.is_some_and(|host| {
                    metadata
                        .host
                        .trim_end_matches('.')
                        .eq_ignore_ascii_case(host.trim_end_matches('.'))
                })))
        && metadata.destination_port.parse::<u16>().ok() == Some(remote.port())
}

fn finish(
    session: &Session,
    outcome: Result<(), String>,
    observed: bool,
    phase: &str,
) -> &'static str {
    if !observed {
        session.step(
            "attempt-1-core",
            "core",
            "unknown",
            "session",
            "未捕获可唯一关联的内核连接；不能判定直连",
            vec!["采集仅覆盖实际请求期间；短连接可能未出现在内核快照中。".into()],
        );
    }
    match outcome {
        Ok(()) => {
            if observed {
                "passed"
            } else {
                "warning"
            }
        }
        Err(error) => {
            session.step(
                &format!("attempt-1-{phase}"),
                phase,
                "failed",
                if phase == "http" { "http" } else { "socket" },
                &error,
                vec![],
            );
            "failed"
        }
    }
}
