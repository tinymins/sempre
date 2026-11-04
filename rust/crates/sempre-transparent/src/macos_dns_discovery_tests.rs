use std::{future::Future, pin::Pin, sync::Mutex};

use super::*;

const TAKEOVER_RESOLVERS: &str =
    "DNS configuration\n\nresolver #1\n  nameserver[0] : 127.0.0.1\n  port     : 20554\n";
const PRIMARY: &str = "<dictionary> {\n  PrimaryService : SERVICE-A\n}\n";
const SERVICE_DNS: &str = "<dictionary> {\n  ServerAddresses : <array> {\n    0 : 223.6.6.6\n    1 : 61.130.254.35\n  }\n}\n";

/// Answers `scutil --dns` and dynamic store reads with fixed output.
struct StoreRunner {
    resolvers: &'static str,
    primary: &'static str,
    service: &'static str,
    calls: Mutex<usize>,
}

impl command::Runner for StoreRunner {
    fn run<'a>(
        &'a self,
        _program: &'a str,
        arguments: &'a [&'a str],
        input: Option<&'a [u8]>,
    ) -> Pin<Box<dyn Future<Output = Result<command::Output, TransparentError>> + Send + 'a>> {
        Box::pin(async move {
            *self.calls.lock().expect("calls") += 1;
            let input = input.map(String::from_utf8_lossy).unwrap_or_default();
            let stdout = if arguments == ["--dns"] {
                self.resolvers
            } else if input.contains("State:/Network/Global/IPv4") {
                self.primary
            } else if input.contains("State:/Network/Service/SERVICE-A/DNS") {
                self.service
            } else {
                "  No such key\n"
            };
            Ok(command::Output {
                success: true,
                stdout: stdout.into(),
                stderr: String::new(),
            })
        })
    }
}

fn store(resolvers: &'static str, primary: &'static str, service: &'static str) -> StoreRunner {
    StoreRunner {
        resolvers,
        primary,
        service,
        calls: Mutex::new(0),
    }
}

fn saved_state(dns: &SystemDns, original_upstreams: Vec<String>) {
    dns.write_state(&State {
        original_upstreams,
        managed_port: 20554,
        services: Vec::new(),
    })
    .expect("ownership state");
}

#[tokio::test]
async fn saved_upstreams_are_used_without_querying_the_system() {
    let root = tempfile::tempdir().expect("temporary directory");
    let dns = SystemDns::new(root.path().into());
    saved_state(&dns, vec!["10.0.0.1".into()]);
    let runner = store(TAKEOVER_RESOLVERS, PRIMARY, SERVICE_DNS);
    let upstreams = dns.discover_upstreams(&runner).await.expect("upstreams");
    assert_eq!(upstreams, ["10.0.0.1"]);
    assert_eq!(*runner.calls.lock().expect("calls"), 0);
}

#[tokio::test]
async fn empty_saved_upstreams_recover_from_the_primary_service_during_takeover() {
    let root = tempfile::tempdir().expect("temporary directory");
    let dns = SystemDns::new(root.path().into());
    saved_state(&dns, Vec::new());
    let runner = store(TAKEOVER_RESOLVERS, PRIMARY, SERVICE_DNS);
    let upstreams = dns.discover_upstreams(&runner).await.expect("upstreams");
    assert_eq!(upstreams, ["223.6.6.6", "61.130.254.35"]);
}

#[tokio::test]
async fn network_not_ready_discovers_no_upstreams_without_failing() {
    let root = tempfile::tempdir().expect("temporary directory");
    let dns = SystemDns::new(root.path().into());
    let runner = store("No DNS configuration available\n", "  No such key\n", "");
    let upstreams = dns.discover_upstreams(&runner).await.expect("upstreams");
    assert!(upstreams.is_empty());
}
