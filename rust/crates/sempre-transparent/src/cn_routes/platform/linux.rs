use serde_json::Value;

use super::{Endpoint, Route, invalid};
use crate::{TransparentError, command};

const TABLE: &str = "20241";
const PROTOCOL: u64 = 253;

async fn json(runner: &dyn command::Runner, args: &[&str]) -> Result<Value, TransparentError> {
    let output = command::require_success("ip", runner.run("ip", args, None).await?)?;
    serde_json::from_str(&output.stdout).map_err(|e| invalid(e.to_string()))
}

#[allow(clippy::unused_async)] // Shared asynchronous platform contract.
pub(super) async fn endpoint(
    _: &dyn command::Runner,
    name: &str,
) -> Result<Endpoint, TransparentError> {
    let index = std::fs::read_to_string(format!("/sys/class/net/{name}/ifindex"))
        .map_err(|e| invalid(e.to_string()))?
        .trim()
        .parse()
        .map_err(|_| invalid("invalid TUN interface index"))?;
    Ok(Endpoint {
        interface: name.into(),
        index,
        gateway: String::new(),
    })
}

pub(super) async fn uplinks(
    runner: &dyn command::Runner,
) -> Result<Vec<Endpoint>, TransparentError> {
    let mut result = Vec::new();
    for family in ["-4", "-6"] {
        let rows = json(
            runner,
            &["-j", family, "route", "show", "default", "table", "main"],
        )
        .await?;
        if let Some(route) = rows
            .as_array()
            .into_iter()
            .flatten()
            .min_by_key(|r| r["metric"].as_u64().unwrap_or(0))
        {
            let name = route["dev"]
                .as_str()
                .ok_or_else(|| invalid("default route has no interface"))?;
            let mut e = endpoint(runner, name).await?;
            e.gateway = route["gateway"]
                .as_str()
                .ok_or_else(|| invalid("default route has no gateway"))?
                .into();
            result.push(e);
        }
    }
    Ok(result)
}

pub(super) async fn preflight(
    runner: &dyn command::Runner,
    _: &[Route],
) -> Result<(), TransparentError> {
    for family in ["-4", "-6"] {
        let output = runner
            .run("ip", &["-j", family, "route", "show", "table", TABLE], None)
            .await?;
        if output.success {
            let routes: Vec<Value> =
                serde_json::from_str(&output.stdout).map_err(|e| invalid(e.to_string()))?;
            if routes.iter().any(|r| !owned(r)) {
                return Err(invalid("CN routing table is owned by another service"));
            }
        } else if !output.stderr.contains("FIB table does not exist") {
            command::require_success("ip", output)?;
        }
        let rules = json(runner, &["-j", family, "rule", "show"]).await?;
        for rule in rules.as_array().into_iter().flatten() {
            let priority = rule["priority"].as_u64().unwrap_or(0);
            if (20241..=20243).contains(&priority) && !owned(rule) {
                return Err(invalid(
                    "CN routing policy priority is owned by another service",
                ));
            }
            if priority > 0 && priority < 20241 {
                return Err(invalid("an earlier policy rule may override CN routing"));
            }
        }
    }
    Ok(())
}

fn owned(value: &Value) -> bool {
    value["protocol"].as_u64() == Some(PROTOCOL) || value["protocol"].as_str() == Some("253")
}

pub(super) async fn change(
    runner: &dyn command::Runner,
    routes: &[Route],
    add: bool,
) -> Result<(), TransparentError> {
    for family in ["-4", "-6"] {
        let mut batch = String::new();
        for route in routes
            .iter()
            .filter(|r| r.prefix.contains(':') == (family == "-6"))
        {
            let gateway = if route.endpoint.gateway.is_empty() {
                String::new()
            } else {
                format!("via {} ", route.endpoint.gateway)
            };
            let operation = if add { "replace" } else { "del" };
            batch.push_str(&format!(
                "route {operation} {} {gateway}dev {} table {TABLE} proto 253\n",
                route.prefix, route.endpoint.interface
            ));
        }
        if batch.is_empty() {
            continue;
        }
        let output = runner
            .run(
                "ip",
                &[family, "-force", "-batch", "-"],
                Some(batch.as_bytes()),
            )
            .await?;
        if !add
            && output.stderr.lines().all(|line| {
                line.contains("No such process")
                    || line.contains("Cannot find device")
                    || line.contains("Command failed")
            })
        {
            continue;
        }
        command::require_success("ip", output)?;
    }
    Ok(())
}

pub(super) async fn policy(
    runner: &dyn command::Runner,
    add: bool,
) -> Result<(), TransparentError> {
    for family in ["-4", "-6"] {
        let rules = json(runner, &["-j", family, "rule", "show"]).await?;
        let mut desired = [
            (
                "20241",
                vec!["fwmark", "0x53500002/0xffffffff", "lookup", "main"],
            ),
            (
                "20242",
                vec!["lookup", "main", "suppress_prefixlength", "0"],
            ),
            ("20243", vec!["lookup", TABLE]),
        ];
        if !add {
            desired.reverse();
        }
        for (priority, tail) in desired {
            let existing = rules.as_array().into_iter().flatten().find(|r| {
                r["priority"].as_u64().map(|p| p.to_string()).as_deref() == Some(priority)
            });
            if let Some(rule) = existing {
                if !owned(rule) {
                    return Err(invalid("refusing to modify another service's policy rule"));
                }
                if add {
                    continue;
                }
            } else if !add {
                continue;
            }
            let mut args = vec![
                family,
                "rule",
                if add { "add" } else { "del" },
                "pref",
                priority,
            ];
            args.extend(tail);
            args.extend(["protocol", "253"]);
            command::require_success("ip", runner.run("ip", &args, None).await?)?;
        }
    }
    Ok(())
}

pub(super) async fn lookup(
    runner: &dyn command::Runner,
    address: &str,
) -> Result<String, TransparentError> {
    let family = if address.contains(':') { "-6" } else { "-4" };
    let rows = json(runner, &["-j", family, "route", "get", address]).await?;
    rows[0]["dev"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("route lookup returned no interface"))
}
