use crate::{DiagnosticFinding, route_probe};

use super::route_evidence;

pub(super) fn runtime_finding() -> DiagnosticFinding {
    DiagnosticFinding {
        code: "runtime_not_running",
        severity: "error",
        title: "Managed proxy core is not running",
        detail: "Transparent proxy and FakeIP traffic cannot be handled while the selected core is stopped.".into(),
        solutions: vec!["Inspect Runtime Status and the latest core failure", "Fix the reported core error before repeating this diagnosis"],
    }
}

pub(super) fn dns_finding(detail: &str) -> DiagnosticFinding {
    DiagnosticFinding {
        code: "dns_failure",
        severity: "error",
        title: "DNS resolution failed",
        detail: detail.into(),
        solutions: vec![
            "Inspect the active system resolver and Sempre DNS status",
            "Verify that the configured upstream DNS is reachable",
        ],
    }
}

pub(super) fn fake_ip_conflict_finding(
    samples: &[route_probe::RouteDecision],
) -> DiagnosticFinding {
    DiagnosticFinding {
        code: "fake_ip_route_conflict",
        severity: "error",
        title: "Multiple TUN routes claim the FakeIP range",
        detail: samples
            .iter()
            .map(route_evidence)
            .collect::<Vec<_>>()
            .join("; "),
        solutions: vec![
            "Do not run two transparent proxy or SASE clients with overlapping FakeIP ranges",
            "If both must remain active, configure a non-overlapping FakeIP range and matching route",
            "Avoid deleting managed routes manually because the owning client may restore them",
        ],
    }
}

pub(super) fn tcp_finding() -> DiagnosticFinding {
    DiagnosticFinding {
        code: "tcp_failure",
        severity: "error",
        title: "TCP connection failed",
        detail: "DNS completed, but none of the resolved addresses accepted a TCP connection."
            .into(),
        solutions: vec![
            "Inspect the selected route, firewall, and proxy-node reachability",
            "Test the selected proxy node and its upstream endpoint",
        ],
    }
}

pub(super) fn tls_finding(route_conflict: bool) -> DiagnosticFinding {
    DiagnosticFinding {
        code: "tls_failure",
        severity: "error",
        title: "TLS handshake failed",
        detail: if route_conflict {
            "TCP reached a FakeIP listener, but the TLS handshake did not return through the same transparent proxy path."
        } else {
            "TCP connected, but the secure handshake did not complete."
        }.into(),
        solutions: vec!["Resolve any reported TUN/FakeIP route conflict first", "Inspect proxy-node health, TLS interception, and path MTU"],
    }
}

pub(super) fn http_finding(detail: &str) -> DiagnosticFinding {
    DiagnosticFinding {
        code: "http_failure",
        severity: "error",
        title: "HTTP request failed",
        detail: detail.into(),
        solutions: vec![
            "Inspect the target response and active routing rule",
            "Compare the target through direct and proxy routes",
        ],
    }
}
