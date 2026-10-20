use std::{net::IpAddr, sync::Arc, time::Duration};

use ipnet::IpNet;
use sempre_dns::{DnsConfig, DnsQueryEvent, DnsRewrite, DnsRuntimePolicy, probe_dns};
use tokio::net::{TcpListener, UdpSocket};

use super::*;

#[derive(Default)]
struct TestPolicy;

impl DnsRuntimePolicy for TestPolicy {
    fn rewrite(&self, _: &str, _: &str) -> Option<DnsRewrite> {
        None
    }

    fn record(&self, _: DnsQueryEvent) {}
}

async fn answering_upstream(
    count: usize,
    address: [u8; 4],
) -> (String, tokio::task::JoinHandle<()>) {
    let upstream = UdpSocket::bind("127.0.0.1:0").await.expect("upstream");
    let socket_address = upstream.local_addr().expect("upstream address");
    let responder = tokio::spawn(async move {
        for _ in 0..count {
            let mut query = [0_u8; 512];
            let (count, peer) = upstream.recv_from(&mut query).await.expect("query");
            let mut response = query[..count].to_vec();
            response[2] |= 0x80;
            response[3] |= 0x80;
            response[6..8].copy_from_slice(&1_u16.to_be_bytes());
            response.extend_from_slice(&[
                0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, address[0], address[1], address[2],
                address[3],
            ]);
            upstream.send_to(&response, peer).await.expect("response");
        }
    });
    (socket_address.to_string(), responder)
}

async fn frontend_port() -> u16 {
    // Windows may reserve different port ranges for UDP and TCP.
    for _ in 0..32 {
        let udp = UdpSocket::bind("127.0.0.1:0").await.expect("UDP port");
        let address = udp.local_addr().expect("frontend address");
        if TcpListener::bind(address).await.is_ok() {
            return address.port();
        }
    }
    panic!("no shared UDP/TCP frontend port available");
}

fn plan(hash: &str, port: u16, local: String, remote: String) -> DnsFrontendPlan {
    DnsFrontendPlan {
        deployment_hash: hash.into(),
        config: DnsConfig::managed_frontend(
            port,
            vec![local],
            remote.clone(),
            Vec::new(),
            &sempre_dns::DomesticDomainPolicy::default(),
        )
        .expect("config"),
        fakeip_enabled: true,
        fakeip_ranges: vec!["198.18.0.0/15".parse::<IpNet>().expect("range")],
        core_upstream: remote,
        original_upstreams: Vec::new(),
    }
}

#[tokio::test]
async fn frontend_starts_and_keeps_domestic_dns_when_core_is_down() {
    for fakeip_enabled in [true, false] {
        let (local, local_task) = answering_upstream(1, [223, 5, 5, 5]).await;
        let dead = UdpSocket::bind("127.0.0.1:0").await.expect("dead port");
        let remote = dead.local_addr().expect("dead address").to_string();
        drop(dead);
        let port = frontend_port().await;
        let mut plan = plan("first", port, local, remote.clone());
        plan.fakeip_enabled = fakeip_enabled;
        let runtime = DnsFrontendRuntime::new(Arc::new(TestPolicy), None);
        runtime
            .prepare(Some(&plan))
            .await
            .expect("prepare frontend");
        tokio::time::timeout(Duration::from_secs(1), runtime.activate_core(Some(&plan)))
            .await
            .expect("activation does not wait for DNS")
            .expect("activate frontend");
        let status = runtime.status();
        assert!(status.running);
        assert!(status.last_error.is_empty());
        assert_eq!(status.core_upstream, remote);
        let domestic = probe_dns(&format!("127.0.0.1:{port}"), "baidu.com", "A")
            .await
            .expect("domestic response");
        assert_eq!(domestic.response_code, 0);
        assert_eq!(
            domestic.addresses,
            ["223.5.5.5".parse::<IpAddr>().expect("IP")]
        );
        local_task.await.expect("local responder");
        runtime.stop().await;
    }
}
