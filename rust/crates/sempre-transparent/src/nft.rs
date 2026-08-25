use std::fmt::Write as _;

use crate::{BYPASS_MARK, Plan, ROUTE_MARK, TransparentError, command};

const TABLE: &str = "sempre_tproxy";
const OWNER_CHAIN: &str = "sempre_owner";
const OWNER_LABEL: &str = "sempre:tproxy:owner:v1";

pub(crate) async fn check_collisions(
    runner: &dyn command::Runner,
) -> Result<Vec<&'static str>, TransparentError> {
    let output =
        command::require_success("nft", runner.run("nft", &["list", "tables"], None).await?)?;
    let mut owned = Vec::new();
    for family in ["ip", "ip6"] {
        if !has_table(&output.stdout, family) {
            continue;
        }
        let output = command::require_success(
            "nft",
            runner
                .run("nft", &["list", "chain", family, TABLE, OWNER_CHAIN], None)
                .await?,
        )?;
        if !output.stdout.contains(OWNER_LABEL) {
            return Err(TransparentError::Invalid(format!(
                "nftables {family} table {TABLE} already exists and is not owned by Sempre"
            )));
        }
        owned.push(family);
    }
    Ok(owned)
}

pub(crate) async fn delete_owned(runner: &dyn command::Runner) -> Result<(), TransparentError> {
    let owned = check_collisions(runner).await?;
    if owned.is_empty() {
        return Ok(());
    }
    let script = owned.into_iter().fold(String::new(), |mut script, family| {
        let _ = writeln!(script, "delete table {family} {TABLE}");
        script
    });
    command::require_success(
        "nft",
        runner
            .run("nft", &["-f", "-"], Some(script.as_bytes()))
            .await?,
    )?;
    Ok(())
}

pub(crate) async fn apply(
    runner: &dyn command::Runner,
    plan: &Plan,
) -> Result<(), TransparentError> {
    let script = script(plan);
    command::require_success(
        "nft",
        runner
            .run("nft", &["-f", "-"], Some(script.as_bytes()))
            .await?,
    )?;
    Ok(())
}

pub(crate) async fn verify(runner: &dyn command::Runner) -> Result<(), TransparentError> {
    let owned = check_collisions(runner).await?;
    if owned == ["ip", "ip6"] {
        Ok(())
    } else {
        Err(TransparentError::Invalid(
            "Sempre TProxy nftables tables are incomplete".into(),
        ))
    }
}

pub(crate) fn script(plan: &Plan) -> String {
    let mut output = String::new();
    for family in ["ip", "ip6"] {
        let address_key = if family == "ip" { "ip" } else { "ip6" };
        let _ = writeln!(output, "add table {family} {TABLE}");
        let _ = writeln!(output, "add chain {family} {TABLE} {OWNER_CHAIN}");
        let _ = writeln!(
            output,
            "add rule {family} {TABLE} {OWNER_CHAIN} counter comment \"{OWNER_LABEL}\""
        );
        let _ = writeln!(
            output,
            "add chain {family} {TABLE} prerouting {{ type filter hook prerouting priority mangle; policy accept; }}"
        );
        for protocol in ["tcp", "udp"] {
            capture_rules(&mut output, family, protocol, plan, true);
        }
        for value in plan
            .excluded_prefixes
            .iter()
            .filter(|value| value.contains(':') == (family == "ip6"))
        {
            let _ = writeln!(
                output,
                "add rule {family} {TABLE} prerouting {address_key} daddr {value} return"
            );
        }
        for protocol in ["tcp", "udp"] {
            capture_rules(&mut output, family, protocol, plan, false);
        }
        if plan.capture_host {
            output_rules(&mut output, family, address_key, plan);
        }
    }
    output
}

fn capture_rules(output: &mut String, family: &str, protocol: &str, plan: &Plan, dns: bool) {
    let port = if dns { plan.dns_port } else { plan.tproxy_port };
    let destination = if dns {
        format!(" {protocol} dport 53")
    } else {
        String::new()
    };
    let kind = if dns { "dns" } else { "proxy" };
    let _ = writeln!(
        output,
        "add rule {family} {TABLE} prerouting meta mark {ROUTE_MARK:#x} meta l4proto {protocol}{destination} meta mark set {ROUTE_MARK:#x} tproxy to :{port} counter accept comment \"sempre:{kind}:host:{protocol}:\""
    );
    for interface in &plan.lan_interfaces {
        let interface = serde_json::to_string(interface).expect("interface string");
        let _ = writeln!(
            output,
            "add rule {family} {TABLE} prerouting iifname {interface} meta l4proto {protocol}{destination} meta mark set {ROUTE_MARK:#x} tproxy to :{port} counter accept comment \"sempre:{kind}:lan:{protocol}:\""
        );
    }
}

fn output_rules(output: &mut String, family: &str, address_key: &str, plan: &Plan) {
    let _ = writeln!(
        output,
        "add chain {family} {TABLE} output {{ type route hook output priority mangle; policy accept; }}"
    );
    let _ = writeln!(
        output,
        "add rule {family} {TABLE} output meta mark {BYPASS_MARK:#x} return"
    );
    for protocol in ["tcp", "udp"] {
        let _ = writeln!(
            output,
            "add rule {family} {TABLE} output meta l4proto {protocol} {protocol} dport 53 meta mark set {ROUTE_MARK:#x} counter accept comment \"sempre:output-dns:host:{protocol}:\""
        );
    }
    for value in plan
        .excluded_prefixes
        .iter()
        .filter(|value| value.contains(':') == (family == "ip6"))
    {
        let _ = writeln!(
            output,
            "add rule {family} {TABLE} output {address_key} daddr {value} return"
        );
    }
    for protocol in ["tcp", "udp"] {
        let _ = writeln!(
            output,
            "add rule {family} {TABLE} output meta l4proto {protocol} meta mark set {ROUTE_MARK:#x} counter accept comment \"sempre:output:host:{protocol}:\""
        );
    }
}

fn has_table(output: &str, family: &str) -> bool {
    output
        .lines()
        .any(|line| line.split_whitespace().eq(["table", family, TABLE]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_owns_dual_stack_tables_and_keeps_bypass_before_output_capture() {
        let plan = Plan {
            tproxy_port: 7893,
            dns_port: 1053,
            capture_host: true,
            lan_interfaces: vec!["vmbr1".into()],
            excluded_prefixes: vec!["192.168.0.0/16".into(), "fc00::/7".into()],
            ..Plan::default()
        };
        let script = script(&plan);
        assert!(script.contains("add table ip sempre_tproxy"));
        assert!(script.contains("add table ip6 sempre_tproxy"));
        assert_eq!(script.matches(OWNER_LABEL).count(), 2);
        assert!(script.contains("iifname \"vmbr1\""));
        assert!(script.contains("tproxy to :7893"));
        assert!(script.contains("tproxy to :1053"));
        assert!(!script.contains("meta l4proto tcp tcp meta"));
        assert!(!script.contains("meta l4proto udp udp meta"));
        let bypass = script
            .find("output meta mark 0x53500002 return")
            .expect("bypass");
        let capture = script
            .find("sempre:output:host:tcp")
            .expect("output capture");
        assert!(bypass < capture);
    }
}
