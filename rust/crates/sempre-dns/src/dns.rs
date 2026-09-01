use std::{io, sync::Arc, time::Duration};

use serde::Serialize;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::watch,
    task::{JoinHandle, JoinSet},
    time::timeout,
};

use crate::model::DnsConfig;
use crate::{
    DnsError,
    dns_policy::{DnsRuntimePolicy, NoopDnsRuntimePolicy},
    resolver::Resolver,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DnsDebugResult {
    pub name: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub upstream: String,
    pub answers: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

pub(crate) struct DnsServer {
    shutdown: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
    resolver: Resolver,
}

impl DnsServer {
    pub(crate) async fn start_with_policy(
        config: DnsConfig,
        policy: Arc<dyn DnsRuntimePolicy>,
    ) -> Result<Self, DnsError> {
        let resolver = Resolver::new(config, policy)?;
        let initial = resolver.config();
        let mut udp = Vec::new();
        let mut tcp = Vec::new();
        for host in &initial.listen_hosts {
            let address = format!("{host}:{}", initial.listen_port);
            udp.push(crate::socket::bind_udp(&address, initial.outbound_mark).await?);
            tcp.push(crate::socket::bind_tcp(&address, initial.outbound_mark).await?);
        }
        let (shutdown, _) = watch::channel(false);
        let mut tasks = Vec::with_capacity(udp.len() + tcp.len());
        for socket in udp {
            tasks.push(tokio::spawn(serve_udp(
                socket,
                resolver.clone(),
                shutdown.subscribe(),
            )));
        }
        for listener in tcp {
            tasks.push(tokio::spawn(serve_tcp(
                listener,
                resolver.clone(),
                shutdown.subscribe(),
            )));
        }
        Ok(Self {
            shutdown,
            tasks,
            resolver,
        })
    }

    pub(crate) fn update(&self, config: DnsConfig) -> Result<(), DnsError> {
        self.resolver.update(config)
    }

    pub(crate) async fn stop(self) {
        let _ = self.shutdown.send(true);
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

pub async fn debug_query(
    config: DnsConfig,
    name: &str,
    record_type: &str,
) -> Result<DnsDebugResult, DnsError> {
    Resolver::new(config, Arc::new(NoopDnsRuntimePolicy))?
        .debug_query(name, record_type)
        .await
}

pub fn managed_probe_names(config: &DnsConfig) -> Result<(String, String), DnsError> {
    let resolver = Resolver::new(config.clone(), Arc::new(NoopDnsRuntimePolicy))?;
    let local = [
        "baidu.com",
        "qq.com",
        "taobao.com",
        "jd.com",
        "bilibili.com",
    ]
    .into_iter()
    .find(|name| resolver.selected_upstream(name) == "local")
    .ok_or_else(|| DnsError::invalid("managed DNS policy has no local probe domain"))?;
    let remote = [
        "example.com",
        "github.com",
        "wikipedia.org",
        "google.com",
        "youtube.com",
    ]
    .into_iter()
    .find(|name| resolver.selected_upstream(name) == "remote")
    .ok_or_else(|| DnsError::invalid("managed DNS policy has no core probe domain"))?;
    Ok((local.into(), remote.into()))
}

async fn serve_udp(socket: UdpSocket, resolver: Resolver, mut shutdown: watch::Receiver<bool>) {
    let socket = Arc::new(socket);
    let mut buffer = vec![0_u8; u16::MAX as usize];
    let mut requests = JoinSet::new();
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    requests.abort_all();
                    while requests.join_next().await.is_some() {}
                    return;
                }
            }
            Some(_) = requests.join_next(), if !requests.is_empty() => {}
            result = socket.recv_from(&mut buffer) => {
                let (count, peer) = match result {
                    Ok(received) => received,
                    Err(error) if retry_udp_receive(&error) => continue,
                    Err(_) => return,
                };
                let request = buffer[..count].to_vec();
                let socket = Arc::clone(&socket);
                let resolver = resolver.clone();
                requests.spawn(async move {
                    let response = resolver.resolve_for_client(&request, peer.ip().to_string()).await;
                    if !response.is_empty() { let _ = socket.send_to(&response, peer).await; }
                });
            }
        }
    }
}

pub(crate) fn retry_udp_receive(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::Interrupted
    )
}

async fn serve_tcp(listener: TcpListener, resolver: Resolver, mut shutdown: watch::Receiver<bool>) {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    connections.abort_all();
                    while connections.join_next().await.is_some() {}
                    return;
                }
            }
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
            accepted = listener.accept() => {
                let Ok((stream, peer)) = accepted else { return; };
                let resolver = resolver.clone();
                connections.spawn(async move { let _ = serve_tcp_connection(stream, resolver, peer.ip().to_string()).await; });
            }
        }
    }
}

async fn serve_tcp_connection(
    mut stream: TcpStream,
    resolver: Resolver,
    client: String,
) -> Result<(), DnsError> {
    let mut length = [0_u8; 2];
    timeout(Duration::from_secs(5), stream.read_exact(&mut length))
        .await
        .map_err(|_| DnsError::invalid("DNS TCP read timed out"))?
        .map_err(|error| DnsError::io("read DNS TCP length", error))?;
    let mut request = vec![0_u8; usize::from(u16::from_be_bytes(length))];
    stream
        .read_exact(&mut request)
        .await
        .map_err(|error| DnsError::io("read DNS TCP query", error))?;
    let response = resolver.resolve_for_client(&request, client).await;
    let length = u16::try_from(response.len())
        .map_err(|_| DnsError::invalid("DNS TCP response exceeds 65535 bytes"))?;
    stream
        .write_all(&length.to_be_bytes())
        .await
        .map_err(|error| DnsError::io("write DNS TCP response length", error))?;
    stream
        .write_all(&response)
        .await
        .map_err(|error| DnsError::io("write DNS TCP response", error))
}

#[cfg(test)]
#[path = "dns_tests.rs"]
mod tests;
