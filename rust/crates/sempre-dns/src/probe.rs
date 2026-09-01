use std::{net::IpAddr, time::Duration};

use tokio::{net::UdpSocket, time::timeout};

use crate::{
    DnsError,
    dns_wire::{answer_ip_addresses, build_query, record_number, response_code},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DnsProbeResult {
    pub response_code: u8,
    pub addresses: Vec<IpAddr>,
}

pub async fn probe_dns(
    upstream: &str,
    name: &str,
    record_type: &str,
) -> Result<DnsProbeResult, DnsError> {
    let (_, record_type) = record_number(record_type)?;
    let request = build_query(name, record_type)?;
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|error| DnsError::io("bind DNS probe socket", error))?;
    socket
        .connect(upstream)
        .await
        .map_err(|error| DnsError::io(format!("connect DNS probe {upstream}"), error))?;
    socket
        .send(&request)
        .await
        .map_err(|error| DnsError::io(format!("send DNS probe to {upstream}"), error))?;
    let mut response = vec![0_u8; u16::MAX as usize];
    let count = timeout(Duration::from_secs(6), socket.recv(&mut response))
        .await
        .map_err(|_| DnsError::invalid(format!("DNS probe {upstream} timed out")))?
        .map_err(|error| DnsError::io(format!("receive DNS probe from {upstream}"), error))?;
    response.truncate(count);
    if response.get(..2) != request.get(..2) {
        return Err(DnsError::invalid(format!(
            "DNS probe {upstream} returned a mismatched transaction"
        )));
    }
    Ok(DnsProbeResult {
        response_code: response_code(&response)?,
        addresses: answer_ip_addresses(&response)?,
    })
}

#[cfg(test)]
mod tests {
    use tokio::net::{TcpListener, UdpSocket};

    use super::*;
    use crate::{DnsConfig, DnsService};

    #[tokio::test]
    async fn core_failure_is_servfail_while_domestic_dns_remains_available() {
        let local = UdpSocket::bind("127.0.0.1:0").await.expect("local DNS");
        let local_address = local.local_addr().expect("local address");
        let responder = tokio::spawn(async move {
            let mut query = [0_u8; 512];
            let (count, peer) = local.recv_from(&mut query).await.expect("query");
            let mut response = query[..count].to_vec();
            response[2] |= 0x80;
            response[3] |= 0x80;
            response[6..8].copy_from_slice(&1_u16.to_be_bytes());
            response.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 223, 5, 5, 5]);
            local.send_to(&response, peer).await.expect("response");
        });
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("frontend port");
        let frontend_port = listener.local_addr().expect("frontend address").port();
        drop(listener);
        let dead = UdpSocket::bind("127.0.0.1:0").await.expect("dead port");
        let dead_address = dead.local_addr().expect("dead address");
        drop(dead);
        let config = DnsConfig::managed_frontend(
            frontend_port,
            vec![local_address.to_string()],
            dead_address.to_string(),
            Vec::new(),
        )
        .expect("config");
        let service = DnsService::start(config).await.expect("frontend");
        let endpoint = format!("127.0.0.1:{frontend_port}");
        let domestic = probe_dns(&endpoint, "baidu.com", "A")
            .await
            .expect("domestic query");
        assert_eq!(domestic.response_code, 0);
        assert_eq!(
            domestic.addresses,
            ["223.5.5.5".parse::<IpAddr>().expect("IP")]
        );
        let proxied = probe_dns(&endpoint, "example.com", "A")
            .await
            .expect("proxied query response");
        assert_eq!(proxied.response_code, 2);
        assert!(proxied.addresses.is_empty());
        responder.await.expect("local responder");
        service.stop().await;
    }
}
