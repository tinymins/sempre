use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UdpSocket},
};

use super::UpstreamClient;
use crate::{DnsConfig, DnsService, dns_wire::build_query, probe_dns};

async fn answer(stream: &mut tokio::net::TcpStream) {
    let length = stream.read_u16().await.expect("query length");
    let mut packet = vec![0; usize::from(length)];
    stream.read_exact(&mut packet).await.expect("query");
    packet[2] |= 0x80;
    // Split the framing and body to exercise stream reads across boundaries.
    for part in [length.to_be_bytes().as_slice(), &packet] {
        stream.write_all(part).await.expect("answer");
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn tcp_reuses_connections_and_reconnects_after_idle_close() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let upstream = format!("tcp://{}", listener.local_addr().expect("address"));
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("first connection");
        answer(&mut stream).await;
        answer(&mut stream).await;
        stream.shutdown().await.expect("close idle");
        drop(stream);
        let (mut stream, _) = listener.accept().await.expect("reconnection");
        answer(&mut stream).await;
    });
    let client = UpstreamClient::default();
    for name in ["one.example", "two.example", "three.example"] {
        let packet = build_query(name, 1).expect("query");
        let response = client
            .exchange(&upstream, &packet, None)
            .await
            .expect("answer");
        assert_eq!(&response[12..], &packet[12..]);
    }
    server.await.expect("server");
}

#[tokio::test]
async fn frontend_falls_back_between_protocols() {
    let tcp = TcpListener::bind("127.0.0.1:0").await.expect("upstream");
    let upstream = format!("tcp://{}", tcp.local_addr().expect("address"));
    let server = tokio::spawn(async move {
        let (mut stream, _) = tcp.accept().await.expect("connection");
        answer(&mut stream).await;
    });
    let port = TcpListener::bind("127.0.0.1:0").await.expect("port");
    let frontend_port = port.local_addr().expect("address").port();
    drop(port);
    let config = DnsConfig::managed_frontend(
        frontend_port,
        vec!["tcp://127.0.0.1:1".into(), upstream],
        "127.0.0.1:1".into(),
        Vec::new(),
    )
    .expect("config");
    let service = DnsService::start(config).await.expect("frontend");
    let response = probe_dns(&format!("127.0.0.1:{frontend_port}"), "baidu.com", "A")
        .await
        .expect("fallback");
    assert_eq!(response.response_code, 0);
    server.await.expect("server");
    service.stop().await;
}

#[tokio::test]
async fn udp_ipv6_and_response_validation() {
    let socket = UdpSocket::bind("[::1]:0").await.expect("IPv6 upstream");
    let upstream = format!("udp://{}", socket.local_addr().expect("address"));
    let server = tokio::spawn(async move {
        let mut packet = [0; 512];
        let (count, peer) = socket.recv_from(&mut packet).await.expect("query");
        packet[2] |= 0x80;
        packet[0] ^= 1;
        socket
            .send_to(&packet[..count], peer)
            .await
            .expect("answer");
    });
    let error = probe_dns(&upstream, "example.com", "A")
        .await
        .expect_err("wrong transaction");
    assert!(error.to_string().contains("mismatched"));
    server.await.expect("server");
}

#[tokio::test]
async fn tls_rejects_untrusted_certificates() {
    use std::sync::Arc;
    use tokio_rustls::{
        TlsAcceptor,
        rustls::{ServerConfig, pki_types::PrivatePkcs8KeyDer},
    };
    let certificate =
        rcgen::generate_simple_self_signed(vec!["localhost".into()]).expect("certificate");
    let config = ServerConfig::builder_with_provider(Arc::new(
        tokio_rustls::rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("versions")
    .with_no_client_auth()
    .with_single_cert(
        vec![certificate.cert.der().clone()],
        PrivatePkcs8KeyDer::from(certificate.key_pair.serialize_der()).into(),
    )
    .expect("TLS server");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let upstream = format!(
        "tls://{}?server_name=localhost",
        listener.local_addr().expect("address")
    );
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("connection");
        assert!(
            TlsAcceptor::from(Arc::new(config))
                .accept(stream)
                .await
                .is_err()
        );
    });
    let error = probe_dns(&upstream, "example.com", "A")
        .await
        .expect_err("untrusted certificate");
    assert!(error.to_string().contains("certificate"), "{error}");
    server.await.expect("server");
}
