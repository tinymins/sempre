use std::{net::SocketAddr, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use futures_util::future::join_all;
use serde::Serialize;
use tokio::{net::TcpStream, time::timeout};
use tokio_rustls::{
    TlsConnector,
    rustls::{ClientConfig, RootCertStore, pki_types::ServerName},
};
use url::Url;

use crate::{NetworkError, dns_probe, route_probe};

mod findings;

use findings::{
    dns_finding, fake_ip_conflict_finding, http_finding, runtime_finding, tcp_finding, tls_finding,
};

const TCP_TIMEOUT: Duration = Duration::from_secs(4);
const TLS_TIMEOUT: Duration = Duration::from_secs(6);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticStatus {
    Passed,
    Failed,
    Warning,
    Skipped,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticLayer {
    pub id: &'static str,
    pub status: DiagnosticStatus,
    pub summary: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticFinding {
    pub code: &'static str,
    pub severity: &'static str,
    pub title: &'static str,
    pub detail: String,
    pub solutions: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NetworkDiagnosticReport {
    pub checked_at: DateTime<Utc>,
    pub target: String,
    pub host: String,
    pub port: u16,
    pub status: DiagnosticStatus,
    pub layers: Vec<DiagnosticLayer>,
    pub findings: Vec<DiagnosticFinding>,
}

#[derive(Clone, Debug)]
struct Attempt {
    address: SocketAddr,
    ok: bool,
    detail: String,
}

pub async fn run_network_diagnostics(
    target: &str,
    runtime_running: bool,
) -> Result<NetworkDiagnosticReport, NetworkError> {
    let (url, host, port) = parse_target(target)?;
    let mut layers = vec![runtime_layer(runtime_running)];
    let mut findings = Vec::new();
    if !runtime_running {
        findings.push(runtime_finding());
    }

    let dns = dns_probe::resolve(&host).await;
    let addresses = dns
        .answers
        .iter()
        .map(|answer| answer.address)
        .collect::<Vec<_>>();
    layers.push(dns_layer(&dns));
    if addresses.is_empty() {
        findings.push(dns_finding(
            dns.error.as_deref().unwrap_or("no addresses returned"),
        ));
        layers.extend(skipped_transport_layers());
        return Ok(report(&url, host, port, layers, findings));
    }

    let (routes, fake_ip_samples) = route_probe::inspect(&addresses);
    let fake_addresses = dns
        .answers
        .iter()
        .filter(|answer| answer.fake_ip)
        .map(|answer| answer.address)
        .collect::<Vec<_>>();
    let mut fake_ip_routes = routes
        .iter()
        .filter(|route| fake_addresses.contains(&route.address))
        .cloned()
        .collect::<Vec<_>>();
    fake_ip_routes.extend(fake_ip_samples);
    let route_conflict = route_probe::fake_ip_routes_conflict(&fake_ip_routes);
    layers.push(route_layer(&routes, &fake_ip_routes, route_conflict));
    if route_conflict {
        findings.push(fake_ip_conflict_finding(&fake_ip_routes));
    }

    let sockets = addresses
        .into_iter()
        .take(4)
        .map(|address| SocketAddr::new(address, port))
        .collect::<Vec<_>>();
    let tcp = join_all(sockets.iter().copied().map(tcp_attempt)).await;
    let tcp_ok = tcp.iter().any(|attempt| attempt.ok);
    layers.push(attempt_layer("tcp", &tcp, tcp_ok));
    if !tcp_ok {
        findings.push(tcp_finding());
        layers.extend(skipped_secure_layers(url.scheme()));
        return Ok(report(&url, host, port, layers, findings));
    }

    if url.scheme() == "https" {
        let tls = join_all(
            sockets
                .iter()
                .copied()
                .map(|address| tls_attempt(address, &host)),
        )
        .await;
        let tls_ok = tls.iter().any(|attempt| attempt.ok);
        layers.push(attempt_layer("tls", &tls, tls_ok));
        if !tls_ok {
            findings.push(tls_finding(route_conflict));
            layers.push(skipped_layer("http", "TLS did not complete"));
            return Ok(report(&url, host, port, layers, findings));
        }
    } else {
        layers.push(skipped_layer("tls", "Target uses plain HTTP"));
    }

    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(HTTP_TIMEOUT)
        .user_agent(concat!("Sempre diagnostics/", env!("CARGO_PKG_VERSION")))
        .build()?;
    match client.get(url.clone()).send().await {
        Ok(response) => layers.push(DiagnosticLayer {
            id: "http",
            status: DiagnosticStatus::Passed,
            summary: format!("HTTP {}", response.status().as_u16()),
            evidence: vec![format!("{:?} {}", response.version(), response.status())],
        }),
        Err(error) => {
            layers.push(DiagnosticLayer {
                id: "http",
                status: DiagnosticStatus::Failed,
                summary: "No HTTP response".into(),
                evidence: vec![error.to_string()],
            });
            findings.push(http_finding(&error.to_string()));
        }
    }
    Ok(report(&url, host, port, layers, findings))
}

fn parse_target(target: &str) -> Result<(Url, String, u16), NetworkError> {
    let url = Url::parse(target).map_err(|error| NetworkError::InvalidTarget(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.host_str().is_none()
    {
        return Err(NetworkError::InvalidTarget(
            "use an HTTP or HTTPS URL without embedded credentials".into(),
        ));
    }
    let host = url.host_str().expect("validated host").to_owned();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| NetworkError::InvalidTarget("target has no known port".into()))?;
    Ok((url, host, port))
}

fn report(
    url: &Url,
    host: String,
    port: u16,
    layers: Vec<DiagnosticLayer>,
    findings: Vec<DiagnosticFinding>,
) -> NetworkDiagnosticReport {
    let status = if layers
        .iter()
        .any(|layer| layer.status == DiagnosticStatus::Failed)
    {
        DiagnosticStatus::Failed
    } else if layers
        .iter()
        .any(|layer| layer.status == DiagnosticStatus::Warning)
    {
        DiagnosticStatus::Warning
    } else {
        DiagnosticStatus::Passed
    };
    NetworkDiagnosticReport {
        checked_at: Utc::now(),
        target: url.to_string(),
        host,
        port,
        status,
        layers,
        findings,
    }
}

fn runtime_layer(running: bool) -> DiagnosticLayer {
    DiagnosticLayer {
        id: "runtime",
        status: if running {
            DiagnosticStatus::Passed
        } else {
            DiagnosticStatus::Failed
        },
        summary: if running {
            "Managed core is running"
        } else {
            "Managed core is not running"
        }
        .into(),
        evidence: Vec::new(),
    }
}

fn dns_layer(dns: &dns_probe::DnsLookup) -> DiagnosticLayer {
    let evidence = dns
        .answers
        .iter()
        .map(|answer| {
            format!(
                "{}{}",
                answer.address,
                if answer.fake_ip { " (FakeIP)" } else { "" }
            )
        })
        .chain(dns.error.iter().cloned())
        .collect();
    DiagnosticLayer {
        id: "dns",
        status: if dns.answers.is_empty() {
            DiagnosticStatus::Failed
        } else {
            DiagnosticStatus::Passed
        },
        summary: if dns.answers.is_empty() {
            "DNS resolution failed".into()
        } else {
            format!("Resolved {} address(es)", dns.answers.len())
        },
        evidence,
    }
}

fn route_layer(
    routes: &[route_probe::RouteDecision],
    samples: &[route_probe::RouteDecision],
    conflict: bool,
) -> DiagnosticLayer {
    let mut evidence = routes.iter().map(route_evidence).collect::<Vec<_>>();
    if conflict {
        evidence.extend(
            samples
                .iter()
                .map(|route| format!("FakeIP sample: {}", route_evidence(route))),
        );
    }
    DiagnosticLayer {
        id: "route",
        status: if conflict {
            DiagnosticStatus::Failed
        } else if routes.is_empty() {
            DiagnosticStatus::Warning
        } else {
            DiagnosticStatus::Passed
        },
        summary: if conflict {
            "Managed FakeIP range is split across multiple routes".into()
        } else if routes.is_empty() {
            "Route ownership is unavailable on this platform".into()
        } else {
            format!("Resolved traffic uses {} route(s)", routes.len())
        },
        evidence,
    }
}

pub(super) fn route_evidence(route: &route_probe::RouteDecision) -> String {
    format!(
        "{} → {}{}",
        route.address,
        route.interface,
        if route.gateway.is_empty() {
            String::new()
        } else {
            format!(" via {}", route.gateway)
        }
    )
}

async fn tcp_attempt(address: SocketAddr) -> Attempt {
    match timeout(TCP_TIMEOUT, TcpStream::connect(address)).await {
        Ok(Ok(_)) => Attempt {
            address,
            ok: true,
            detail: "connected".into(),
        },
        Ok(Err(error)) => Attempt {
            address,
            ok: false,
            detail: error.to_string(),
        },
        Err(_) => Attempt {
            address,
            ok: false,
            detail: "timed out".into(),
        },
    }
}

async fn tls_attempt(address: SocketAddr, host: &str) -> Attempt {
    let stream = match timeout(TCP_TIMEOUT, TcpStream::connect(address)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            return Attempt {
                address,
                ok: false,
                detail: error.to_string(),
            };
        }
        Err(_) => {
            return Attempt {
                address,
                ok: false,
                detail: "TCP timed out".into(),
            };
        }
    };
    let roots = webpki_roots::TLS_SERVER_ROOTS
        .iter()
        .cloned()
        .collect::<RootCertStore>();
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let Ok(server_name) = ServerName::try_from(host.to_owned()) else {
        return Attempt {
            address,
            ok: false,
            detail: "invalid TLS server name".into(),
        };
    };
    match timeout(
        TLS_TIMEOUT,
        TlsConnector::from(Arc::new(config)).connect(server_name, stream),
    )
    .await
    {
        Ok(Ok(_)) => Attempt {
            address,
            ok: true,
            detail: "handshake completed".into(),
        },
        Ok(Err(error)) => Attempt {
            address,
            ok: false,
            detail: error.to_string(),
        },
        Err(_) => Attempt {
            address,
            ok: false,
            detail: "handshake timed out".into(),
        },
    }
}

fn attempt_layer(id: &'static str, attempts: &[Attempt], ok: bool) -> DiagnosticLayer {
    DiagnosticLayer {
        id,
        status: if ok {
            DiagnosticStatus::Passed
        } else {
            DiagnosticStatus::Failed
        },
        summary: if ok {
            format!("{id} completed")
        } else {
            format!("{id} failed")
        },
        evidence: attempts
            .iter()
            .map(|attempt| format!("{}: {}", attempt.address, attempt.detail))
            .collect(),
    }
}

fn skipped_layer(id: &'static str, reason: &str) -> DiagnosticLayer {
    DiagnosticLayer {
        id,
        status: DiagnosticStatus::Skipped,
        summary: reason.into(),
        evidence: Vec::new(),
    }
}

fn skipped_transport_layers() -> Vec<DiagnosticLayer> {
    ["route", "tcp", "tls", "http"]
        .map(|id| skipped_layer(id, "DNS must succeed first"))
        .into()
}

fn skipped_secure_layers(scheme: &str) -> Vec<DiagnosticLayer> {
    vec![
        skipped_layer(
            "tls",
            if scheme == "https" {
                "TCP must succeed first"
            } else {
                "Target uses plain HTTP"
            },
        ),
        skipped_layer("http", "TCP must succeed first"),
    ]
}
