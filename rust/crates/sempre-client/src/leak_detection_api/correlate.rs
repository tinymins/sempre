use std::net::IpAddr;

use sempre_core_control::{ConnectionSnapshot, Proxy};

use super::{
    model::{Connection, Input},
    platform::{PlatformSnapshot, SocketInfo},
};

fn address(ip: &str, port: u16) -> String {
    if ip.contains(':') {
        format!("[{ip}]:{port}")
    } else {
        format!("{ip}:{port}")
    }
}

fn fake_ip(ip: &str, ranges: &[ipnet::IpNet]) -> bool {
    ip.parse::<IpAddr>()
        .is_ok_and(|ip| ranges.iter().any(|range| range.contains(&ip)))
}

fn family(ip: &str, ranges: &[ipnet::IpNet]) -> &'static str {
    match ip.parse::<IpAddr>() {
        Ok(IpAddr::V4(_)) if !fake_ip(ip, ranges) => "ipv4",
        Ok(IpAddr::V6(_)) if !fake_ip(ip, ranges) => "ipv6",
        _ => "unknown",
    }
}

fn matches(
    socket: &SocketInfo,
    connection: &sempre_core_control::Connection,
    ranges: &[ipnet::IpNet],
) -> bool {
    let metadata = &connection.metadata;
    socket.protocol.eq_ignore_ascii_case(&metadata.network)
        && socket.local_port.to_string() == metadata.source_port
        && socket.remote_port.to_string() == metadata.destination_port
        && socket
            .local_ip
            .parse::<IpAddr>()
            .ok()
            .is_some_and(|ip| Some(ip) == metadata.source_ip.parse::<IpAddr>().ok())
        && (socket
            .remote_ip
            .parse::<IpAddr>()
            .ok()
            .is_some_and(|ip| Some(ip) == metadata.destination_ip.parse::<IpAddr>().ok())
            || (fake_ip(&socket.remote_ip, ranges)
                && metadata.dns_mode.eq_ignore_ascii_case("fake-ip")))
}

fn core_evidence(
    record: &mut Connection,
    connection: &sempre_core_control::Connection,
    proxies: &[Proxy],
    ranges: &[ipnet::IpNet],
) {
    record.chains.clone_from(&connection.chains);
    record.rule.clone_from(&connection.rule);
    record.host.clone_from(&connection.metadata.host);
    record.id = format!(
        "{}:core:{}:{}:{}",
        record.socket_key,
        connection.id,
        connection.rule,
        serde_json::to_string(&connection.chains).unwrap_or_default()
    );
    // Resolve actual leaf types: arbitrary user-defined outbound names are not evidence.
    let types: Vec<_> = connection
        .chains
        .iter()
        .filter_map(|name| {
            proxies
                .iter()
                .find(|proxy| proxy.name == *name)
                .map(|proxy| proxy.proxy_type.to_lowercase())
        })
        .collect();
    record.verdict = if types.iter().any(|kind| kind == "direct") {
        "direct-route"
    } else if types
        .iter()
        .any(|kind| ["reject", "rejectdrop", "reject-drop", "pass"].contains(&kind.as_str()))
    {
        "unattributed"
    } else if types.iter().any(|kind| {
        [
            "ss",
            "shadowsocks",
            "ssr",
            "shadowsocksr",
            "vmess",
            "vless",
            "trojan",
            "hysteria",
            "hysteria2",
            "tuic",
            "wireguard",
            "socks",
            "socks5",
            "http",
            "snell",
            "ssh",
            "mieru",
            "anytls",
        ]
        .contains(&kind.as_str())
    }) {
        "proxy-route"
    } else {
        "unattributed"
    };
    record.evidence.push(format!(
        "内核连接 {}；仅证明当前路由选择，不等同于物理出口已验证",
        connection.id
    ));
    if !connection.metadata.destination_ip.is_empty() {
        record.evidence.push(format!(
            "内核目标地址族：{}；应用套接字地址族另行记录",
            family(&connection.metadata.destination_ip, ranges)
        ));
    }
}

pub(super) fn connections(
    input: &Input,
    platform: &PlatformSnapshot,
    core: Option<&ConnectionSnapshot>,
    proxies: &[Proxy],
    ranges: &[ipnet::IpNet],
) -> Vec<Connection> {
    if input.topology == "gateway" {
        return core
            .into_iter()
            .flat_map(|snapshot| &snapshot.connections)
            .filter(|connection| {
                connection.metadata.source_ip.parse::<IpAddr>().ok()
                    == input.client_ip.as_ref().and_then(|ip| ip.parse().ok())
            })
            .map(|connection| {
                let metadata = &connection.metadata;
                let mut record = Connection {
                    id: format!("core:{}", connection.id),
                    socket_key: format!("core:{}", connection.id),
                    pid: None,
                    app: "网关客户端（应用未归属）".into(),
                    protocol: metadata.network.clone(),
                    source: address(
                        &metadata.source_ip,
                        metadata.source_port.parse().unwrap_or(0),
                    ),
                    destination: address(
                        &metadata.destination_ip,
                        metadata.destination_port.parse().unwrap_or(0),
                    ),
                    host: String::new(),
                    ip_version: family(&metadata.destination_ip, ranges),
                    verdict: "unattributed",
                    rule: String::new(),
                    chains: vec![],
                    physical_egress: false,
                    evidence: vec!["按客户端源 IP 筛选；无法仅凭网关流量确定客户端进程".into()],
                };
                core_evidence(&mut record, connection, proxies, ranges);
                record
            })
            .collect();
    }
    platform
        .sockets
        .iter()
        .map(|socket| {
            let process = platform
                .processes
                .iter()
                .find(|process| process.pid == socket.pid);
            let mut record = Connection {
                id: format!(
                    "{}:{}:{}:{}:{}:{}:{}",
                    socket.pid,
                    process.map_or(0, |process| process.started_at),
                    socket.protocol,
                    socket.local_ip,
                    socket.local_port,
                    socket.remote_ip,
                    socket.remote_port
                ),
                socket_key: String::new(),
                pid: Some(socket.pid),
                app: process
                    .map_or("unknown", |process| process.app.as_str())
                    .into(),
                protocol: socket.protocol.clone(),
                source: address(&socket.local_ip, socket.local_port),
                destination: address(&socket.remote_ip, socket.remote_port),
                host: String::new(),
                ip_version: family(&socket.remote_ip, ranges),
                verdict: "unattributed",
                rule: String::new(),
                chains: vec![],
                physical_egress: false,
                evidence: vec!["操作系统进程套接字；未关联到内核不代表绕过代理".into()],
            };
            record.socket_key.clone_from(&record.id);
            if fake_ip(&socket.remote_ip, ranges) {
                record
                    .evidence
                    .push("应用目标是运行时 FakeIP；该地址不是公网出口".into());
            }
            let candidates: Vec<_> = core
                .into_iter()
                .flat_map(|snapshot| &snapshot.connections)
                .filter(|connection| matches(socket, connection, ranges))
                .take(2)
                .collect();
            if let [connection] = candidates.as_slice() {
                record
                    .evidence
                    .push("按协议、源地址/端口与目标端口关联；FakeIP 使用运行时地址范围".into());
                core_evidence(&mut record, connection, proxies, ranges);
            } else if candidates.len() > 1 {
                record
                    .evidence
                    .push("匹配到多条内核连接，归属存在歧义".into());
            }
            record
        })
        .collect()
}
