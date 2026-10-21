use std::{net::SocketAddr, time::Duration};

use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _},
    net::TcpStream,
    time::timeout,
};
use tokio_rustls::{TlsConnector, rustls::pki_types::ServerName};
use url::Url;

use crate::NetworkError;

#[derive(Clone, Debug)]
pub enum TraceTransportEvent {
    Connected {
        local: SocketAddr,
        remote: SocketAddr,
    },
    Stage {
        phase: &'static str,
        complete: bool,
        detail: String,
    },
}

pub fn parse_trace_target(target: &str) -> Result<(Url, String, u16), NetworkError> {
    crate::diagnostics::parse_target(target)
}

/// One socket, TLS session and HTTP request. Redirects are reported, not followed.
pub async fn trace_transport<F>(url: &Url, address: SocketAddr, mut emit: F) -> Result<(), String>
where
    F: FnMut(TraceTransportEvent),
{
    let stream = timeout(Duration::from_secs(5), TcpStream::connect(address))
        .await
        .map_err(|_| "TCP 连接超过 5 秒未建立".to_owned())?
        .map_err(|e| e.to_string())?;
    emit(TraceTransportEvent::Connected {
        local: stream.local_addr().map_err(|e| e.to_string())?,
        remote: stream.peer_addr().map_err(|e| e.to_string())?,
    });
    if url.scheme() == "https" {
        emit(TraceTransportEvent::Stage {
            phase: "tls",
            complete: false,
            detail: "正在同一次 TCP 连接上协商 TLS".into(),
        });
        let name = ServerName::try_from(
            url.host_str()
                .unwrap_or_default()
                .trim_matches(['[', ']'])
                .to_owned(),
        )
        .map_err(|e| e.to_string())?;
        let connector = TlsConnector::from(crate::diagnostics::tls::tls_config());
        let stream = timeout(Duration::from_secs(6), connector.connect(name, stream))
            .await
            .map_err(|_| "TLS 握手超过 6 秒未完成".to_owned())?
            .map_err(|e| e.to_string())?;
        emit(TraceTransportEvent::Stage {
            phase: "tls",
            complete: true,
            detail: "TLS 握手已完成，证书验证通过".into(),
        });
        http(stream, url, &mut emit).await
    } else {
        http(stream, url, &mut emit).await
    }
}

async fn http<S, F>(mut stream: S, url: &Url, emit: &mut F) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: FnMut(TraceTransportEvent),
{
    emit(TraceTransportEvent::Stage {
        phase: "request",
        complete: false,
        detail: "正在向已建立的连接写入本次 HTTP GET 请求".into(),
    });
    let mut path = url.path().to_owned();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    let host = &url[url::Position::BeforeHost..url::Position::AfterPort];
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: Sempre-diagnostics\r\nConnection: close\r\n\r\n"
    );
    let mut waiting_response = false;
    let response = timeout(Duration::from_secs(10), async {
        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        stream.flush().await.map_err(|e| e.to_string())?;
        emit(TraceTransportEvent::Stage {
            phase: "request",
            complete: true,
            detail: "本次请求已完整写入连接".into(),
        });
        waiting_response = true;
        emit(TraceTransportEvent::Stage {
            phase: "http",
            complete: false,
            detail: "请求已发送，正在等待目标返回完整响应头".into(),
        });
        let mut headers = Vec::new();
        let mut chunk = [0_u8; 1024];
        let mut received = 0;
        loop {
            if let Some(end) = headers.windows(4).position(|part| part == b"\r\n\r\n") {
                let line = headers[..end]
                    .split(|byte| *byte == b'\n')
                    .next()
                    .unwrap_or_default();
                let status = String::from_utf8_lossy(line).trim().to_owned();
                let mut fields = status.split_whitespace();
                let version = fields.next().unwrap_or_default();
                let code = fields.next().and_then(|value| value.parse::<u16>().ok());
                if !matches!(version, "HTTP/1.0" | "HTTP/1.1")
                    || !code.is_some_and(|value| (100..600).contains(&value))
                {
                    return Err("目标返回了无效的 HTTP 状态行".into());
                }
                match code {
                    Some(101) => return Err("目标要求切换协议，无法完成本次 HTTP 检查".into()),
                    Some(200..=599) => return Ok::<String, String>(status),
                    _ => {
                        headers.drain(..end + 4);
                    }
                }
            } else {
                let count = stream.read(&mut chunk).await.map_err(|e| e.to_string())?;
                if count == 0 {
                    return Err("目标在完整响应头返回前关闭了连接".into());
                }
                received += count;
                if received > 16 * 1024 {
                    return Err("目标响应头超过 16 KiB，已停止读取".into());
                }
                headers.extend_from_slice(&chunk[..count]);
            }
        }
    })
    .await
    .map_err(|_| {
        if waiting_response {
            "等待目标响应超时：本次 HTTP 请求已达到 10 秒时限"
        } else {
            "发送请求超时：10 秒内未完成请求写入"
        }
        .to_owned()
    })??;
    emit(TraceTransportEvent::Stage {
        phase: "http",
        complete: true,
        detail: format!("已收到目标响应头 · {response}"),
    });
    Ok(())
}
