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
        config: DnsConfig::managed_frontend(port, vec![local], remote.clone(), Vec::new())
            .expect("config"),
        fakeip_enabled: true,
        fakeip_ranges: vec!["198.18.0.0/15".parse::<IpNet>().expect("range")],
        core_upstream: remote,
        original_upstreams: Vec::new(),
        local_probe: "baidu.com".into(),
        remote_probe: "example.com".into(),
    }
}

#[tokio::test]
async fn frontend_starts_and_keeps_domestic_dns_when_core_is_down() {
    let (local, local_task) = answering_upstream(2, [223, 5, 5, 5]).await;
    let dead = UdpSocket::bind("127.0.0.1:0").await.expect("dead port");
    let remote = dead.local_addr().expect("dead address").to_string();
    drop(dead);
    let port = frontend_port().await;
    let plan = plan("first", port, local, remote);
    let runtime = DnsFrontendRuntime::new(Arc::new(TestPolicy), None);

    runtime
        .prepare(Some(&plan), Duration::from_millis(200))
        .await
        .expect("prepare frontend");
    runtime
        .activate_core(Some(&plan), Duration::from_millis(10))
        .await;

    let status = runtime.status();
    assert!(status.running);
    assert!(!status.core_dns_healthy);
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

#[tokio::test]
async fn healthy_candidate_promotes_new_core_upstream_without_stopping_frontend() {
    let (local, local_task) = answering_upstream(4, [223, 5, 5, 5]).await;
    let (first, first_task) = answering_upstream(2, [198, 18, 0, 1]).await;
    let (second, second_task) = answering_upstream(2, [198, 18, 0, 2]).await;
    let port = frontend_port().await;
    let first_plan = plan("first", port, local.clone(), first);
    let second_plan = plan("second", port, local, second.clone());
    let runtime = DnsFrontendRuntime::new(Arc::new(TestPolicy), None);

    runtime
        .prepare(Some(&first_plan), Duration::from_millis(200))
        .await
        .expect("prepare frontend");
    runtime
        .activate_core(Some(&first_plan), Duration::from_secs(1))
        .await;
    runtime
        .prepare(Some(&second_plan), Duration::from_millis(200))
        .await
        .expect("retain frontend");
    runtime
        .activate_core(Some(&second_plan), Duration::from_secs(1))
        .await;

    let status = runtime.status();
    assert!(status.running && status.core_dns_healthy);
    assert_eq!(status.core_upstream, second);

    local_task.await.expect("local responder");
    first_task.await.expect("first responder");
    second_task.await.expect("second responder");
    runtime.stop().await;
}

#[tokio::test]
async fn unhealthy_candidate_keeps_the_last_healthy_core_upstream() {
    let (local, local_task) = answering_upstream(3, [223, 5, 5, 5]).await;
    let (first, first_task) = answering_upstream(3, [198, 18, 0, 1]).await;
    let dead = UdpSocket::bind("127.0.0.1:0").await.expect("dead port");
    let second = dead.local_addr().expect("dead address").to_string();
    drop(dead);
    let port = frontend_port().await;
    let first_plan = plan("first", port, local.clone(), first.clone());
    let second_plan = plan("second", port, local, second);
    let runtime = DnsFrontendRuntime::new(Arc::new(TestPolicy), None);

    runtime
        .prepare(Some(&first_plan), Duration::from_millis(200))
        .await
        .expect("prepare frontend");
    runtime
        .activate_core(Some(&first_plan), Duration::from_secs(1))
        .await;
    runtime
        .prepare(Some(&second_plan), Duration::from_millis(200))
        .await
        .expect("retain frontend");
    runtime
        .activate_core(Some(&second_plan), Duration::from_millis(10))
        .await;

    let status = runtime.status();
    assert!(status.running);
    assert!(!status.core_dns_healthy);
    assert_eq!(status.core_upstream, first);
    let remote = probe_dns(&format!("127.0.0.1:{port}"), "example.com", "A")
        .await
        .expect("retained remote response");
    assert_eq!(
        remote.addresses,
        ["198.18.0.1".parse::<IpAddr>().expect("IP")]
    );

    local_task.await.expect("local responder");
    first_task.await.expect("first responder");
    runtime.stop().await;
}

#[tokio::test]
async fn changes_upstreams_while_core_is_down_without_rebinding() {
    let (first, first_task) = answering_upstream(1, [223, 5, 5, 5]).await;
    let (second, second_task) = answering_upstream(1, [223, 6, 6, 6]).await;
    let port = frontend_port().await;
    let plan = plan("same-core", port, first, "127.0.0.1:1".into());
    let runtime = DnsFrontendRuntime::new(Arc::new(TestPolicy), None);
    runtime
        .prepare(Some(&plan), Duration::from_millis(200))
        .await
        .expect("frontend");
    let upstreams = vec![format!("udp://{second}")];
    runtime.update_upstreams(&upstreams).await.expect("update");
    assert_eq!(runtime.status().direct_upstreams, upstreams);
    assert!(!runtime.status().core_dns_healthy);
    let reply = probe_dns(&format!("127.0.0.1:{port}"), "baidu.com", "A")
        .await
        .expect("new upstream");
    assert_eq!(
        reply.addresses,
        ["223.6.6.6".parse::<IpAddr>().expect("IP")]
    );
    first_task.await.expect("first");
    second_task.await.expect("second");
    runtime.stop().await;
}
