use super::*;
use crate::DnsRewrite;
use std::sync::Mutex as StdMutex;

#[derive(Default)]
struct TestPolicy {
    rewrite: Option<DnsRewrite>,
    events: StdMutex<Vec<DnsQueryEvent>>,
}

impl DnsRuntimePolicy for TestPolicy {
    fn rewrite(&self, _name: &str, record_type: &str) -> Option<DnsRewrite> {
        self.rewrite
            .clone()
            .filter(|rule| rule.record_type == record_type)
    }

    fn record(&self, event: DnsQueryEvent) {
        self.events.lock().expect("events").push(event);
    }
}

async fn answering_upstream(count: usize, address: [u8; 4]) -> (String, JoinHandle<()>) {
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

#[tokio::test]
async fn debug_query_exchanges_with_the_selected_udp_upstream() {
    let upstream = UdpSocket::bind("127.0.0.1:0").await.expect("upstream");
    let address = upstream.local_addr().expect("upstream address");
    let responder = tokio::spawn(async move {
        let mut query = [0_u8; 512];
        let (count, peer) = upstream.recv_from(&mut query).await.expect("query");
        let mut response = query[..count].to_vec();
        response[2] |= 0x80;
        response[3] |= 0x80;
        response[6..8].copy_from_slice(&1_u16.to_be_bytes());
        response.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 10, 0, 0, 1]);
        upstream.send_to(&response, peer).await.expect("response");
    });
    let config = DnsConfig {
        local_upstreams: vec![address.to_string()],
        remote_upstream: address.to_string(),
        ..DnsConfig::default()
    };
    let result = debug_query(config, "example.com", "A")
        .await
        .expect("debug query");
    responder.await.expect("responder");
    assert_eq!(result.upstream, address.to_string());
    assert!(result.answers[0].ends_with("A 10.0.0.1"));
}

#[tokio::test]
async fn managed_frontend_enforces_proxy_direct_domestic_then_default_order() {
    let (local, local_task) = answering_upstream(2, [10, 0, 0, 1]).await;
    let (remote, remote_task) = answering_upstream(2, [198, 18, 0, 1]).await;
    let config = DnsConfig::managed_frontend(
        1054,
        vec![local],
        remote,
        vec!["domain,proxy.baidu.com".into()],
        vec!["domain,direct.example".into()],
        false,
    )
    .expect("managed frontend");
    for (name, answer, detail) in [
        ("proxy.baidu.com", "198.18.0.1", "rule-set:explicit-proxy"),
        ("direct.example", "10.0.0.1", "rule-set:explicit-direct"),
        ("baidu.com", "10.0.0.1", "rule-set:domestic-domains"),
        ("github.com", "198.18.0.1", "default-remote"),
    ] {
        let result = debug_query(config.clone(), name, "A")
            .await
            .expect("debug query");
        assert!(result.answers[0].ends_with(&format!("A {answer}")));
        assert_eq!(result.detail, detail);
    }
    local_task.await.expect("local responder");
    remote_task.await.expect("remote responder");
}

#[tokio::test]
async fn rewrite_precedes_split_routing_and_records_the_decision() {
    let policy = Arc::new(TestPolicy {
        rewrite: Some(DnsRewrite {
            id: "local-test".into(),
            enabled: true,
            domain: "example.com".into(),
            record_type: "A".into(),
            answer: "10.23.0.1".into(),
            ttl: 60,
            comment: String::new(),
        }),
        events: StdMutex::new(Vec::new()),
    });
    let resolver = Resolver::new(
        DnsConfig::default(),
        Arc::clone(&policy) as Arc<dyn DnsRuntimePolicy>,
    )
    .expect("resolver");
    let query = build_query("example.com", 1).expect("query");
    let response = resolver
        .resolve_for_client(&query, "127.0.0.1".into())
        .await;
    assert!(format_answers(&response).expect("answers")[0].ends_with("A 10.23.0.1"));
    let events = policy.events.lock().expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].decision, "rewrite");
    assert_eq!(events[0].detail, "rewrite:local-test");
}
