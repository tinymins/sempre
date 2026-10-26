use std::net::{IpAddr, SocketAddr};

use super::{
    model::Report,
    platform::{CaptureSnapshot, DnsObservation, PacketInfo},
};

fn public_destination(address: IpAddr, ranges: &[ipnet::IpNet]) -> bool {
    if ranges.iter().any(|range| range.contains(&address)) {
        return false;
    }
    match address {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_broadcast()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_documentation()
                && a != 0
                && a < 240
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 198 && (b == 18 || b == 19))
        }
        IpAddr::V6(ip) => {
            ip.segments()[0] & 0xe000 == 0x2000
                && !(ip.segments()[0] == 0x2001 && ip.segments()[1] == 0x0db8)
        }
    }
}

fn endpoint(ip: &str, port: u16) -> Option<SocketAddr> {
    ip.parse().ok().map(|ip| SocketAddr::new(ip, port))
}

fn physical(packet: &PacketInfo) -> bool {
    ["ethernet", "wifi"].contains(&packet.interface_kind.as_str()) && packet.direction == "out"
}

pub(super) fn merge(
    report: &mut Report,
    snapshot: CaptureSnapshot,
    ranges: &[ipnet::IpNet],
    current_ids: &[String],
) {
    report.capture.status = snapshot.status;
    report.capture.detail = snapshot.detail;
    for packet in snapshot.packets {
        if report.capture.packets.len() >= 400 {
            report.gap("接口包头记录达到 400 条上限；额外包头未保留或关联");
            break;
        }
        if physical(&packet) {
            if report.topology == "local"
                && packet
                    .destination_ip
                    .parse()
                    .is_ok_and(|ip| public_destination(ip, ranges))
            {
                let source = endpoint(&packet.source_ip, packet.source_port);
                let destination = endpoint(&packet.destination_ip, packet.destination_port);
                for connection in &mut report.connections {
                    if current_ids.contains(&connection.id)
                        && source.is_some()
                        && destination.is_some()
                        && source == connection.source.parse().ok()
                        && destination == connection.destination.parse().ok()
                        && connection.protocol.eq_ignore_ascii_case(&packet.protocol)
                        && !connection.physical_egress
                    {
                        connection.physical_egress = true;
                        connection.evidence.push(format!("{} 出站包头与应用套接字一致；可能是显式代理端点，不能据此断言模型服务收到真实 IP", packet.interface));
                    }
                }
            }
            if matches!(packet.destination_port, 53 | 853) && report.dns.len() < 200 {
                let protocol = format!("{}/{} candidate", packet.protocol, packet.destination_port);
                if !report.dns.iter().any(|dns| {
                    dns.scope == "device"
                        && dns.protocol == protocol
                        && dns.server == packet.destination_ip
                }) {
                    report.dns.push(DnsObservation {
                        scope: "device".into(), protocol, server: packet.destination_ip.clone(),
                        evidence: format!("物理接口 {} 观察到发往 DNS 常用端口的包头；未读取查询内容，不能据此归属应用或确认 DNS 泄漏", packet.interface),
                    });
                }
            }
        }
        report.capture.packets.push(packet);
    }
}
