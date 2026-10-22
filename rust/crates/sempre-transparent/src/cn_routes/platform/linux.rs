use serde_json::Value;
use std::fmt::Write;

use super::{Endpoint, Route, invalid};
use crate::{TransparentError, command};

const TABLE: &str = "20241";
const CAPTURE_TABLE: &str = "20244";
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
    desired: &[Route],
) -> Result<(), TransparentError> {
    let capture = desired.iter().any(|route| route.scoped);
    for family in ["-4", "-6"] {
        for table in [Some(TABLE), capture.then_some(CAPTURE_TABLE)]
            .into_iter()
            .flatten()
        {
            let output = runner
                .run("ip", &["-j", family, "route", "show", "table", table], None)
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
        }
        let rules = json(runner, &["-j", family, "rule", "show"]).await?;
        for rule in rules.as_array().into_iter().flatten() {
            let priority = rule["priority"].as_u64().unwrap_or(0);
            if ((20241..=20243).contains(&priority) || (capture && priority == 20239))
                && !owned(rule)
            {
                return Err(invalid(
                    "CN routing policy priority is owned by another service",
                ));
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
            let table = if route.scoped { CAPTURE_TABLE } else { TABLE };
            let _ = writeln!(
                batch,
                "route {operation} {} {gateway}dev {} table {table} proto 253",
                route.prefix, route.endpoint.interface
            );
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
    capture: bool,
) -> Result<(), TransparentError> {
    for family in ["-4", "-6"] {
        let rules = json(runner, &["-j", family, "rule", "show"]).await?;
        let mut desired = [
            (
                "20239",
                capture,
                vec![
                    "not",
                    "fwmark",
                    "0x53500002/0xffffffff",
                    "lookup",
                    CAPTURE_TABLE,
                ],
            ),
            (
                "20241",
                true,
                vec!["fwmark", "0x53500002/0xffffffff", "lookup", "main"],
            ),
            (
                "20242",
                true,
                vec!["lookup", "main", "suppress_prefixlength", "0"],
            ),
            ("20243", true, vec!["lookup", TABLE]),
        ];
        if !add {
            desired.reverse();
        }
        for (priority, enabled, tail) in desired {
            let install = add && enabled;
            let existing = rules.as_array().into_iter().flatten().find(|r| {
                r["priority"].as_u64().map(|p| p.to_string()).as_deref() == Some(priority)
            });
            if let Some(rule) = existing {
                if !owned(rule) {
                    if priority == "20239" && !install {
                        continue;
                    }
                    return Err(invalid("refusing to modify another service's policy rule"));
                }
                if install {
                    continue;
                }
            } else if !install {
                continue;
            }
            let mut args = vec![
                family,
                "rule",
                if install { "add" } else { "del" },
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
