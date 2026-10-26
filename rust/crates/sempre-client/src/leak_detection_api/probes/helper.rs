use super::types::Message;
use std::{
    io::{BufRead, Read, Write},
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpStream, lookup_host},
};
use tokio_rustls::{
    TlsConnector,
    rustls::{ClientConfig, RootCertStore, pki_types::ServerName},
};

pub(crate) fn main(kind: &str) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let result = match runtime {
        Ok(runtime) => runtime.block_on(run(kind)),
        Err(_) => Err("探针运行时不可用".into()),
    };
    if let Err(detail) = result {
        let _ = emit(&Message::Error { detail });
    }
}
fn emit(message: &Message) -> Result<(), String> {
    let encoded = serde_json::to_string(message).map_err(|_| "无法编码探针结果")?;
    let mut output = std::io::stdout().lock();
    writeln!(output, "{encoded}")
        .and_then(|()| output.flush())
        .map_err(|_| "探针输出已关闭".into())
}
fn gate(expected: &str) -> Result<(), String> {
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .take(32)
        .read_line(&mut line)
        .map_err(|_| "探针控制已关闭")?;
    if line.trim() == expected {
        Ok(())
    } else {
        Err("探针已取消或控制通道已关闭".into())
    }
}
async fn run(kind: &str) -> Result<(), String> {
    emit(&Message::Ready)?;
    gate("start")?;
    let host = if kind == "ipv6" {
        "api6.ipify.org"
    } else {
        "api.ipify.org"
    };
    let addresses: Vec<SocketAddr> =
        tokio::time::timeout(Duration::from_secs(4), lookup_host((host, 443)))
            .await
            .map_err(|_| "系统 DNS 查询超时")?
            .map_err(|_| "系统 DNS 查询失败")?
            .take(16)
            .collect();
    if kind == "dns" {
        return emit(&Message::Result {
            addresses: addresses
                .iter()
                .map(|address| address.ip().to_string())
                .collect(),
            public_ip: None,
            detail:
                "系统解析已返回；缓存可能命中。没有权威 DNS 观测，不能证明解析出口或无 DNS 泄漏"
                    .into(),
        });
    }
    let remote = addresses
        .iter()
        .find(|address| address.is_ipv6() == (kind == "ipv6"))
        .copied()
        .ok_or("系统解析未返回所需地址族；该地址族没有完成测试")?;
    let stream = tokio::time::timeout(Duration::from_secs(4), TcpStream::connect(remote))
        .await
        .map_err(|_| "TCP 连接超时")?
        .map_err(|_| "TCP 连接失败")?;
    let local = stream.local_addr().map_err(|_| "无法读取真实源端口")?;
    let remote = stream.peer_addr().map_err(|_| "无法读取真实对端")?;
    emit(&Message::Connected {
        local,
        remote,
        phase: "tcp".into(),
    })?;
    gate("continue")?;
    let config = ClientConfig::builder_with_provider(Arc::new(
        tokio_rustls::rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| "TLS 协议配置失败")?
    .with_root_certificates(
        webpki_roots::TLS_SERVER_ROOTS
            .iter()
            .cloned()
            .collect::<RootCertStore>(),
    )
    .with_no_client_auth();
    let name = ServerName::try_from(host.to_owned()).map_err(|_| "TLS 主机名无效")?;
    let mut tls = TlsConnector::from(Arc::new(config))
        .connect(name, stream)
        .await
        .map_err(|_| "TLS 握手或证书验证失败")?;
    emit(&Message::Connected {
        local,
        remote,
        phase: "tls".into(),
    })?;
    gate("continue")?;
    let request = format!(
        "GET /?format=json HTTP/1.0\r\nHost: {host}\r\nUser-Agent: Sempre-leak-probe\r\nConnection: close\r\n\r\n"
    );
    tls.write_all(request.as_bytes())
        .await
        .map_err(|_| "HTTPS 请求写入失败")?;
    let response = super::http::read_response(&mut tls).await?;
    let body = super::http::body(&response)?;
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| "公网 IP 服务未返回有效 JSON")?;
    let ip: IpAddr = value
        .get("ip")
        .and_then(serde_json::Value::as_str)
        .and_then(|ip| ip.parse().ok())
        .ok_or("公网 IP 字段无效")?;
    if ip.is_ipv6() != (kind == "ipv6") {
        return Err("公网 IP 返回地址族与实际连接族不一致".into());
    }
    emit(&Message::Result {
        addresses: vec![remote.to_string()], public_ip: Some(ip.to_string()),
        detail: "独立探针进程在同一 TCP/TLS 连接取得外部 IP；这不代表 Codex/Claude Code 的出口，且不证明 DNS 路径".into(),
    })
}
