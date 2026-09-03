//! Native capture smoke fixture using the production resolver and TLS transport.
use std::{io::Write, sync::Arc};

use sempre_dns::{
    DnsConfig, DnsQueryEvent, DnsRewrite, DnsRuntimePolicy, DnsService, default_upstreams,
};

struct FixturePolicy;

impl DnsRuntimePolicy for FixturePolicy {
    fn rewrite(&self, name: &str, record_type: &str) -> Option<DnsRewrite> {
        (name.ends_with(".sempre.invalid.") && record_type == "A").then(|| DnsRewrite {
            enabled: true,
            domain: name.into(),
            record_type: "A".into(),
            answer: "203.0.113.11".into(),
            ..DnsRewrite::default()
        })
    }

    fn record(&self, event: DnsQueryEvent) {
        println!(
            "QUERY {} {} {} {}",
            event.name,
            event.upstream,
            event.answers.join(";"),
            event.error
        );
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let upstreams = std::env::args()
        .nth(1)
        .map_or_else(default_upstreams, |value| {
            value.split(',').map(str::to_owned).collect()
        });
    let config =
        DnsConfig::managed_frontend(port, upstreams.clone(), upstreams[0].clone(), Vec::new())?;
    let service = DnsService::start_with_policy(config, Arc::new(FixturePolicy)).await?;
    println!("READY {port}");
    std::io::stdout().flush()?;
    tokio::task::spawn_blocking(|| std::io::stdin().read_line(&mut String::new())).await??;
    service.stop().await;
    Ok(())
}
