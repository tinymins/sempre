use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

async fn server(statuses: Vec<u16>) -> (&'static str, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Box::leak(format!("http://{}/", listener.local_addr().unwrap()).into_boxed_str());
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        for (index, status) in statuses.into_iter().enumerate() {
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(socket.read_u8().await.unwrap());
            }
            assert!(request.starts_with(b"GET / HTTP/1.1"));
            if index == 0 {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            socket
                .write_all(
                    format!("HTTP/1.1 {status} Test\r\nContent-Length: 2\r\n\r\nok").as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    (url, task)
}

fn probe(url: &'static str) -> Probe {
    Probe {
        url,
        parse: None,
        success: status_200,
        ..PROBES[2]
    }
}

fn client() -> Client {
    Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap()
}

#[tokio::test]
async fn measures_repeat_request_on_the_same_connection_independently_of_dns() {
    let (url, server) = server(vec![200, 200]).await;
    let client = client();
    // The DNS display branch can finish later without inflating either HTTP measurement.
    let ((), result) = tokio::join!(
        tokio::time::sleep(Duration::from_secs(1)),
        run_http_probe(&client, probe(url)),
    );
    assert!(result.ok);
    let repeat = result.response_latency_ms.expect("repeat request latency");
    assert!(result.latency_ms >= 300);
    assert!(result.latency_ms < 900);
    assert!(repeat + 150 < result.latency_ms);
    server.await.unwrap();
}

#[tokio::test]
async fn failed_repeat_does_not_replace_latency_with_first_request_time() {
    let (url, server) = server(vec![200, 503]).await;
    let result = run_http_probe(&client(), probe(url)).await;
    assert!(result.ok);
    assert_eq!(result.http_status, Some(200));
    assert!(result.response_latency_ms.is_none());
    assert!(result.latency_ms >= 300);
    server.await.unwrap();
}

#[tokio::test]
async fn failed_first_request_is_not_retried_as_a_latency_sample() {
    let (url, server) = server(vec![503]).await;
    let result = run_http_probe(&client(), probe(url)).await;
    assert!(!result.ok);
    assert_eq!(result.http_status, Some(503));
    assert!(result.response_latency_ms.is_none());
    server.await.unwrap();
}
