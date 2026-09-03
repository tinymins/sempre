use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time::{Instant, timeout},
};
use tokio_rustls::{
    TlsConnector,
    rustls::{ClientConfig, RootCertStore},
};

use crate::{
    DnsError,
    dns_wire::{answer_ip_addresses, build_query},
    socket,
    upstream_endpoint::{Endpoint, Protocol},
};

trait DnsStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> DnsStream for T {}
type Stream = Box<dyn DnsStream>;
type Pool = HashMap<(String, Option<u32>), Vec<(Instant, Stream)>>;

#[derive(Default)]
pub(crate) struct UpstreamClient {
    idle: Mutex<Pool>,
}

impl UpstreamClient {
    pub async fn exchange(
        &self,
        upstream: &str,
        packet: &[u8],
        mark: Option<u32>,
    ) -> Result<Vec<u8>, DnsError> {
        timeout(
            Duration::from_secs(5),
            self.exchange_inner(upstream, packet, mark),
        )
        .await
        .map_err(|_| DnsError::invalid(format!("DNS upstream {upstream} timed out")))?
    }

    async fn exchange_inner(
        &self,
        upstream: &str,
        packet: &[u8],
        mark: Option<u32>,
    ) -> Result<Vec<u8>, DnsError> {
        let endpoint = Endpoint::parse(upstream)?;
        if endpoint.protocol == Protocol::Udp {
            let address = self.addresses(&endpoint, mark).await?[0];
            let socket = socket::upstream_socket(address, mark)?;
            socket
                .connect(address)
                .await
                .map_err(|error| DnsError::io("connect DNS upstream", error))?;
            socket
                .send(packet)
                .await
                .map_err(|error| DnsError::io("send DNS query", error))?;
            let mut response = vec![0; usize::from(u16::MAX)];
            let count = socket
                .recv(&mut response)
                .await
                .map_err(|error| DnsError::io("receive DNS answer", error))?;
            response.truncate(count);
            validate_response(packet, &response)?;
            return Ok(response);
        }
        let key = (upstream.to_owned(), mark);
        let idle = {
            let mut pool = self.idle.lock().expect("DNS upstream pool");
            let entries = pool.entry(key.clone()).or_default();
            entries.retain(|(time, _)| time.elapsed() < Duration::from_secs(30));
            entries.pop().map(|(_, stream)| stream)
        };
        let mut stream = if let Some(mut stream) = idle {
            // Servers may close idle connections. Only retry a previously pooled stream.
            if let Ok(response) = exchange_stream(stream.as_mut(), packet).await {
                self.recycle(key, stream);
                return Ok(response);
            }
            connect(&endpoint, self.addresses(&endpoint, mark).await?, mark).await?
        } else {
            connect(&endpoint, self.addresses(&endpoint, mark).await?, mark).await?
        };
        let response = exchange_stream(stream.as_mut(), packet).await?;
        self.recycle(key, stream);
        Ok(response)
    }

    async fn addresses(
        &self,
        endpoint: &Endpoint,
        mark: Option<u32>,
    ) -> Result<Vec<SocketAddr>, DnsError> {
        if let Ok(ip) = endpoint.host.parse::<IpAddr>() {
            return Ok(vec![SocketAddr::new(ip, endpoint.port)]);
        }
        // Resolve upstream hostnames through IP-addressed DoT, never the intercepted OS resolver.
        for record_type in [1, 28] {
            let query = build_query(&endpoint.host, record_type)?;
            for bootstrap in crate::default_upstreams() {
                if let Ok(reply) = Box::pin(self.exchange(&bootstrap, &query, mark)).await {
                    let addresses = answer_ip_addresses(&reply)?;
                    if !addresses.is_empty() {
                        return Ok(addresses
                            .into_iter()
                            .map(|ip| SocketAddr::new(ip, endpoint.port))
                            .collect());
                    }
                }
            }
        }
        Err(DnsError::invalid(format!(
            "DoT bootstrap returned no address for {}",
            endpoint.host
        )))
    }

    fn recycle(&self, key: (String, Option<u32>), stream: Stream) {
        let mut pool = self.idle.lock().expect("DNS upstream pool");
        let entries = pool.entry(key).or_default();
        if entries.len() < 4 {
            entries.push((Instant::now(), stream));
        }
    }
}

async fn connect(
    endpoint: &Endpoint,
    addresses: Vec<SocketAddr>,
    mark: Option<u32>,
) -> Result<Stream, DnsError> {
    let mut last_error = DnsError::invalid("DNS upstream has no address");
    for address in addresses {
        let stream = match socket::upstream_tcp(address, mark).await {
            Ok(stream) => stream,
            Err(error) => {
                last_error = error;
                continue;
            }
        };
        if endpoint.protocol == Protocol::Tcp {
            return Ok(Box::new(stream));
        }
        let stream = TlsConnector::from(tls_config())
            .connect(endpoint.server_name.clone(), stream)
            .await
            .map_err(|error| DnsError::io("authenticate DNS TLS upstream", error))?;
        return Ok(Box::new(stream));
    }
    Err(last_error)
}

fn tls_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    Arc::clone(CONFIG.get_or_init(|| {
        let roots = webpki_roots::TLS_SERVER_ROOTS
            .iter()
            .cloned()
            .collect::<RootCertStore>();
        Arc::new(
            ClientConfig::builder_with_provider(Arc::new(
                tokio_rustls::rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .expect("TLS protocol versions")
            .with_root_certificates(roots)
            .with_no_client_auth(),
        )
    }))
}

async fn exchange_stream(stream: &mut dyn DnsStream, packet: &[u8]) -> Result<Vec<u8>, DnsError> {
    let length =
        u16::try_from(packet.len()).map_err(|_| DnsError::invalid("DNS query is too large"))?;
    let mut frame = Vec::with_capacity(packet.len() + 2);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(packet);
    stream
        .write_all(&frame)
        .await
        .map_err(|error| DnsError::io("send DNS stream query", error))?;
    stream
        .flush()
        .await
        .map_err(|error| DnsError::io("flush DNS stream query", error))?;
    let length = stream
        .read_u16()
        .await
        .map_err(|error| DnsError::io("read DNS stream length", error))?;
    let mut response = vec![0; usize::from(length)];
    stream
        .read_exact(&mut response)
        .await
        .map_err(|error| DnsError::io("read DNS stream answer", error))?;
    validate_response(packet, &response)?;
    Ok(response)
}

fn validate_response(request: &[u8], response: &[u8]) -> Result<(), DnsError> {
    if response.len() < 12 || response.get(..2) != request.get(..2) || response[2] & 0x80 == 0 {
        return Err(DnsError::invalid(
            "DNS upstream returned an invalid or mismatched response",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "upstream_tests.rs"]
mod tests;
