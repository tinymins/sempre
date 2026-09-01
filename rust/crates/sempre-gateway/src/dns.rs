use std::{net::Ipv4Addr, sync::Arc, time::Duration};

use serde::Serialize;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{TcpListener, TcpStream, UdpSocket, lookup_host},
    sync::watch,
    task::{JoinHandle, JoinSet},
    time::timeout,
};

use crate::model::DnsConfig;
use crate::{
    GatewayError,
    dns_wire::{
        TYPE_HTTPS, answer_ipv4_addresses, build_query, format_answers, fqdn, parse_question,
        record_number, response_with_code,
    },
    domain_matcher::DomainMatcher,
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
}

#[derive(Clone)]
struct Resolver {
    config: DnsConfig,
    domestic_cidrs: Vec<(Ipv4Addr, u8)>,
    rule_sets: Vec<ResolvedRuleSet>,
}

#[derive(Clone)]
struct ResolvedRuleSet {
    id: String,
    upstream: String,
    matcher: DomainMatcher,
}

struct Resolved {
    packet: Vec<u8>,
    upstream: String,
    detail: String,
}

impl DnsServer {
    pub(crate) async fn start(config: DnsConfig) -> Result<Self, GatewayError> {
        let resolver = Resolver::new(config)?;
        let mut udp = Vec::new();
        let mut tcp = Vec::new();
        for host in &resolver.config.listen_hosts {
            let address = format!("{host}:{}", resolver.config.listen_port);
            udp.push(
                UdpSocket::bind(&address).await.map_err(|error| {
                    GatewayError::io(format!("listen DNS UDP {address}"), error)
                })?,
            );
            tcp.push(
                TcpListener::bind(&address).await.map_err(|error| {
                    GatewayError::io(format!("listen DNS TCP {address}"), error)
                })?,
            );
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
        Ok(Self { shutdown, tasks })
    }

    pub(crate) async fn stop(self) {
        let _ = self.shutdown.send(true);
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

pub(crate) async fn debug_query(
    config: DnsConfig,
    name: &str,
    record_type: &str,
) -> Result<DnsDebugResult, GatewayError> {
    Resolver::new(config)?.debug_query(name, record_type).await
}

pub fn managed_probe_names(config: &DnsConfig) -> Result<(String, String), GatewayError> {
    let resolver = Resolver::new(config.clone())?;
    let local = [
        "baidu.com",
        "qq.com",
        "taobao.com",
        "jd.com",
        "bilibili.com",
    ]
    .into_iter()
    .find(|name| resolver.selected_upstream(name) == "local")
    .ok_or_else(|| GatewayError::invalid("managed DNS policy has no local probe domain"))?;
    let remote = [
        "example.com",
        "github.com",
        "wikipedia.org",
        "google.com",
        "youtube.com",
    ]
    .into_iter()
    .find(|name| resolver.selected_upstream(name) == "remote")
    .ok_or_else(|| GatewayError::invalid("managed DNS policy has no core probe domain"))?;
    Ok((local.into(), remote.into()))
}

impl Resolver {
    fn new(config: DnsConfig) -> Result<Self, GatewayError> {
        let domestic_cidrs = config
            .domestic_cidrs
            .iter()
            .map(|value| parse_cidr(value))
            .collect::<Result<Vec<_>, _>>()?;
        let rule_sets = config
            .rule_sets
            .iter()
            .filter(|rule_set| rule_set.enabled)
            .map(|rule_set| ResolvedRuleSet {
                id: rule_set.id.clone(),
                upstream: rule_set.upstream.clone(),
                matcher: DomainMatcher::from_rules(&rule_set.rules),
            })
            .collect();
        Ok(Self {
            config,
            domestic_cidrs,
            rule_sets,
        })
    }

    async fn debug_query(
        &self,
        name: &str,
        record_type: &str,
    ) -> Result<DnsDebugResult, GatewayError> {
        let (record_type, number) = record_number(record_type)?;
        let packet = build_query(name, number)?;
        let resolved = self.resolve(&packet).await?;
        Ok(DnsDebugResult {
            name: fqdn(name),
            record_type,
            upstream: resolved.upstream,
            answers: format_answers(&resolved.packet)?,
            detail: resolved.detail,
        })
    }

    async fn resolve(&self, packet: &[u8]) -> Result<Resolved, GatewayError> {
        let question = parse_question(packet)?;
        let Some(question) = question else {
            return Ok(Resolved {
                packet: response_with_code(packet, 0)?,
                upstream: String::new(),
                detail: "empty-question".into(),
            });
        };
        if self.config.reject_https && question.record_type == TYPE_HTTPS {
            return Ok(Resolved {
                packet: response_with_code(packet, 3)?,
                upstream: "reject".into(),
                detail: "https-rejected".into(),
            });
        }
        if let Some(rule_set) = self.match_rule(&question.name) {
            let mut resolved = self.exchange_named(packet, &rule_set.upstream).await?;
            resolved.detail = format!("rule-set:{}", rule_set.id);
            return Ok(resolved);
        }
        if self.config.strategy == "rules-first" {
            let mut resolved = self.exchange(packet, &self.config.remote_upstream).await?;
            resolved.detail = "default-remote".into();
            return Ok(resolved);
        }
        let local = self.exchange_local(packet).await;
        if local
            .as_ref()
            .is_ok_and(|response| self.domestic_response(&response.packet))
        {
            return local.map(|mut response| {
                response.detail = "local-response".into();
                response
            });
        }
        match self.exchange(packet, &self.config.remote_upstream).await {
            Ok(mut remote) => {
                remote.detail = "remote-response".into();
                Ok(remote)
            }
            Err(remote_error) => local.or(Err(remote_error)),
        }
    }

    async fn exchange_named(
        &self,
        packet: &[u8],
        upstream: &str,
    ) -> Result<Resolved, GatewayError> {
        match upstream {
            "local" => self.exchange_local(packet).await,
            "remote" | "" => self.exchange(packet, &self.config.remote_upstream).await,
            upstream => self.exchange(packet, upstream).await,
        }
    }

    async fn exchange_local(&self, packet: &[u8]) -> Result<Resolved, GatewayError> {
        if self.config.local_upstreams.is_empty() {
            return self.exchange(packet, &self.config.remote_upstream).await;
        }
        let mut last_error = None;
        for upstream in &self.config.local_upstreams {
            match self.exchange(packet, upstream).await {
                Ok(response) => return Ok(response),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.expect("non-empty local upstreams"))
    }

    async fn exchange(&self, packet: &[u8], upstream: &str) -> Result<Resolved, GatewayError> {
        let address = lookup_host(upstream)
            .await
            .map_err(|error| GatewayError::io(format!("resolve DNS upstream {upstream}"), error))?
            .next()
            .ok_or_else(|| GatewayError::invalid(format!("DNS upstream {upstream:?} is empty")))?;
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|error| GatewayError::io("bind DNS upstream socket", error))?;
        socket
            .connect(address)
            .await
            .map_err(|error| GatewayError::io(format!("connect DNS upstream {upstream}"), error))?;
        socket
            .send(packet)
            .await
            .map_err(|error| GatewayError::io(format!("send DNS query to {upstream}"), error))?;
        let mut response = vec![0_u8; u16::MAX as usize];
        let count = timeout(Duration::from_secs(5), socket.recv(&mut response))
            .await
            .map_err(|_| GatewayError::invalid(format!("DNS upstream {upstream} timed out")))?
            .map_err(|error| {
                GatewayError::io(format!("receive DNS response from {upstream}"), error)
            })?;
        response.truncate(count);
        if response.get(..2) != packet.get(..2) {
            return Err(GatewayError::invalid(format!(
                "DNS upstream {upstream} returned a mismatched transaction"
            )));
        }
        Ok(Resolved {
            packet: response,
            upstream: upstream.into(),
            detail: String::new(),
        })
    }

    fn match_rule(&self, name: &str) -> Option<&ResolvedRuleSet> {
        self.rule_sets
            .iter()
            .find(|rule_set| rule_set.matcher.matches(name))
    }

    fn selected_upstream(&self, name: &str) -> &str {
        self.match_rule(name)
            .map_or("remote", |rule_set| rule_set.upstream.as_str())
    }

    fn domestic_response(&self, packet: &[u8]) -> bool {
        answer_ipv4_addresses(packet).is_ok_and(|addresses| {
            addresses.into_iter().any(|address| {
                self.domestic_cidrs
                    .iter()
                    .any(|(network, prefix)| in_prefix(address, *network, *prefix))
            })
        })
    }
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
                let Ok((count, peer)) = result else { return; };
                let request = buffer[..count].to_vec();
                let socket = Arc::clone(&socket);
                let resolver = resolver.clone();
                requests.spawn(async move {
                    let response = resolver.resolve(&request).await
                        .map_or_else(|_| response_with_code(&request, 2).unwrap_or_default(), |value| value.packet);
                    if !response.is_empty() { let _ = socket.send_to(&response, peer).await; }
                });
            }
        }
    }
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
                let Ok((stream, _)) = accepted else { return; };
                let resolver = resolver.clone();
                connections.spawn(async move { let _ = serve_tcp_connection(stream, resolver).await; });
            }
        }
    }
}

async fn serve_tcp_connection(
    mut stream: TcpStream,
    resolver: Resolver,
) -> Result<(), GatewayError> {
    let mut length = [0_u8; 2];
    timeout(Duration::from_secs(5), stream.read_exact(&mut length))
        .await
        .map_err(|_| GatewayError::invalid("DNS TCP read timed out"))?
        .map_err(|error| GatewayError::io("read DNS TCP length", error))?;
    let mut request = vec![0_u8; usize::from(u16::from_be_bytes(length))];
    stream
        .read_exact(&mut request)
        .await
        .map_err(|error| GatewayError::io("read DNS TCP query", error))?;
    let response = resolver.resolve(&request).await.map_or_else(
        |_| response_with_code(&request, 2).unwrap_or_default(),
        |value| value.packet,
    );
    let length = u16::try_from(response.len())
        .map_err(|_| GatewayError::invalid("DNS TCP response exceeds 65535 bytes"))?;
    stream
        .write_all(&length.to_be_bytes())
        .await
        .map_err(|error| GatewayError::io("write DNS TCP response length", error))?;
    stream
        .write_all(&response)
        .await
        .map_err(|error| GatewayError::io("write DNS TCP response", error))
}

fn parse_cidr(value: &str) -> Result<(Ipv4Addr, u8), GatewayError> {
    let (address, prefix) = value
        .split_once('/')
        .ok_or_else(|| GatewayError::invalid(format!("invalid IPv4 prefix {value:?}")))?;
    let address = address
        .parse()
        .map_err(|_| GatewayError::invalid(format!("invalid IPv4 prefix {value:?}")))?;
    let prefix = prefix
        .parse::<u8>()
        .ok()
        .filter(|prefix| *prefix <= 32)
        .ok_or_else(|| GatewayError::invalid(format!("invalid IPv4 prefix {value:?}")))?;
    Ok((address, prefix))
}

fn in_prefix(address: Ipv4Addr, network: Ipv4Addr, prefix: u8) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    u32::from(address) & mask == u32::from(network) & mask
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
