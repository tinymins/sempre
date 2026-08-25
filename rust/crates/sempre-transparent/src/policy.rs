use serde_json::Value;

use crate::{POLICY_PROTOCOL, ROUTE_MARK, ROUTE_TABLE, RULE_PRIORITY, TransparentError, command};

pub(crate) async fn check_collisions(runner: &dyn command::Runner) -> Result<(), TransparentError> {
    for family in ["-4", "-6"] {
        let rules = json_command(runner, &["-j", family, "rule", "show"]).await?;
        for rule in rules.as_array().into_iter().flatten() {
            let priority = number(rule, "priority");
            let table = number(rule, "table");
            if (priority == Some(u64::from(RULE_PRIORITY)) || table == Some(u64::from(ROUTE_TABLE)))
                && !owned_rule(rule)
            {
                return Err(TransparentError::Invalid(format!(
                    "policy rule priority {RULE_PRIORITY} or table {ROUTE_TABLE} is owned by another service"
                )));
            }
        }
        let routes = route_json(runner, &["-j", family, "route", "show", "table", "20240"]).await?;
        for route in routes.as_array().into_iter().flatten() {
            if !owned_route(route, family) {
                return Err(TransparentError::Invalid(format!(
                    "route table {ROUTE_TABLE} contains routes not owned by Sempre"
                )));
            }
        }
    }
    Ok(())
}

pub(crate) async fn apply(runner: &dyn command::Runner) -> Result<(), TransparentError> {
    for family in ["-4", "-6"] {
        let destination = if family == "-4" { "0.0.0.0/0" } else { "::/0" };
        run(
            runner,
            &[
                family,
                "route",
                "add",
                "local",
                destination,
                "dev",
                "lo",
                "table",
                "20240",
                "proto",
                "253",
            ],
        )
        .await?;
        if let Err(error) = run(
            runner,
            &[
                family,
                "rule",
                "add",
                "pref",
                "20240",
                "fwmark",
                "0x53500001/0xffffffff",
                "lookup",
                "20240",
                "protocol",
                "253",
            ],
        )
        .await
        {
            let _ = delete(runner).await;
            return Err(error);
        }
    }
    Ok(())
}

pub(crate) async fn delete(runner: &dyn command::Runner) -> Result<(), TransparentError> {
    let mut failure = None;
    for family in ["-4", "-6"] {
        let rules = json_command(runner, &["-j", family, "rule", "show"]).await?;
        if rules
            .as_array()
            .is_some_and(|values| values.iter().any(owned_rule))
            && let Err(error) = run(
                runner,
                &[
                    family,
                    "rule",
                    "del",
                    "pref",
                    "20240",
                    "fwmark",
                    "0x53500001/0xffffffff",
                    "lookup",
                    "20240",
                    "protocol",
                    "253",
                ],
            )
            .await
        {
            failure = Some(error);
        }
        let routes = route_json(runner, &["-j", family, "route", "show", "table", "20240"]).await?;
        if routes
            .as_array()
            .is_some_and(|values| values.iter().any(|route| owned_route(route, family)))
        {
            let destination = if family == "-4" { "0.0.0.0/0" } else { "::/0" };
            if let Err(error) = run(
                runner,
                &[
                    family,
                    "route",
                    "del",
                    "local",
                    destination,
                    "dev",
                    "lo",
                    "table",
                    "20240",
                    "proto",
                    "253",
                ],
            )
            .await
            {
                failure = Some(error);
            }
        }
    }
    failure.map_or(Ok(()), Err)
}

pub(crate) async fn verify(runner: &dyn command::Runner) -> Result<(), TransparentError> {
    for family in ["-4", "-6"] {
        let rules = json_command(runner, &["-j", family, "rule", "show"]).await?;
        if !rules
            .as_array()
            .is_some_and(|values| values.iter().any(owned_rule))
        {
            return Err(TransparentError::Invalid(format!(
                "Sempre {family} policy rule is missing"
            )));
        }
        let routes = route_json(runner, &["-j", family, "route", "show", "table", "20240"]).await?;
        if !routes
            .as_array()
            .is_some_and(|values| values.iter().any(|route| owned_route(route, family)))
        {
            return Err(TransparentError::Invalid(format!(
                "Sempre {family} policy route is missing"
            )));
        }
    }
    Ok(())
}

async fn json_command(
    runner: &dyn command::Runner,
    arguments: &[&str],
) -> Result<Value, TransparentError> {
    let output = command::require_success("ip", runner.run("ip", arguments, None).await?)?;
    serde_json::from_str(&output.stdout)
        .map_err(|error| TransparentError::Invalid(format!("decode ip command JSON: {error}")))
}

async fn route_json(
    runner: &dyn command::Runner,
    arguments: &[&str],
) -> Result<Value, TransparentError> {
    let output = runner.run("ip", arguments, None).await?;
    let missing = !output.success && output.stderr.contains("FIB table does not exist");
    if missing {
        return Ok(Value::Array(Vec::new()));
    }
    if !output.success {
        return match command::require_success("ip", output) {
            Err(error) => Err(error),
            Ok(_) => unreachable!("route output was unsuccessful"),
        };
    }
    serde_json::from_str(&output.stdout)
        .map_err(|error| TransparentError::Invalid(format!("decode ip route JSON: {error}")))
}

async fn run(runner: &dyn command::Runner, arguments: &[&str]) -> Result<(), TransparentError> {
    command::require_success("ip", runner.run("ip", arguments, None).await?)?;
    Ok(())
}

fn owned_rule(rule: &Value) -> bool {
    number(rule, "priority") == Some(u64::from(RULE_PRIORITY))
        && number(rule, "table") == Some(u64::from(ROUTE_TABLE))
        && mark(rule) == Some(u64::from(ROUTE_MARK))
        && number(rule, "protocol") == Some(u64::from(POLICY_PROTOCOL))
}

fn owned_route(route: &Value, family: &str) -> bool {
    let destination = route.get("dst").and_then(Value::as_str);
    let default = destination == Some("default")
        || destination == Some(if family == "-4" { "0.0.0.0/0" } else { "::/0" });
    route.get("type").and_then(Value::as_str) == Some("local")
        && default
        && number(route, "protocol") == Some(u64::from(POLICY_PROTOCOL))
}

fn mark(rule: &Value) -> Option<u64> {
    rule.get("fwmark").and_then(|value| {
        value.as_u64().or_else(|| {
            value.as_str().and_then(|value| {
                u64::from_str_radix(value.trim_start_matches("0x").split('/').next()?, 16).ok()
            })
        })
    })
}

fn number(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
    })
}

#[cfg(test)]
mod tests {
    use std::{future::Future, pin::Pin};

    use super::*;
    use crate::command::Output;
    use serde_json::json;

    struct MissingRoutes;

    impl command::Runner for MissingRoutes {
        fn run<'a>(
            &'a self,
            _: &'a str,
            arguments: &'a [&'a str],
            _: Option<&'a [u8]>,
        ) -> Pin<Box<dyn Future<Output = Result<Output, TransparentError>> + Send + 'a>> {
            Box::pin(async move {
                let route = arguments.contains(&"route");
                Ok(Output {
                    success: !route,
                    stdout: if route { "[".into() } else { "[]\n".into() },
                    stderr: if route {
                        "Error: ipv4: FIB table does not exist.\nDump terminated\n".into()
                    } else {
                        String::new()
                    },
                })
            })
        }
    }

    #[test]
    fn ownership_requires_all_discriminators() {
        let owned = json!({
            "priority": 20240, "table": 20240, "fwmark": "0x53500001/0xffffffff",
            "protocol": 253
        });
        assert!(owned_rule(&owned));
        let mut foreign = owned;
        foreign["protocol"] = json!(4);
        assert!(!owned_rule(&foreign));
        assert!(owned_route(
            &json!({ "type": "local", "dst": "default", "protocol": 253 }),
            "-4"
        ));
    }

    #[tokio::test]
    async fn missing_kernel_route_tables_are_empty_owned_state() {
        check_collisions(&MissingRoutes)
            .await
            .expect("missing table has no collision");
        delete(&MissingRoutes)
            .await
            .expect("cleanup ignores missing table");
    }

    struct FailedRoutes;

    impl command::Runner for FailedRoutes {
        fn run<'a>(
            &'a self,
            _: &'a str,
            arguments: &'a [&'a str],
            _: Option<&'a [u8]>,
        ) -> Pin<Box<dyn Future<Output = Result<Output, TransparentError>> + Send + 'a>> {
            Box::pin(async move {
                Ok(Output {
                    success: !arguments.contains(&"route"),
                    stdout: String::new(),
                    stderr: "RTNETLINK answers: Operation not permitted\n".into(),
                })
            })
        }
    }

    #[tokio::test]
    async fn route_query_failures_are_not_hidden() {
        assert!(check_collisions(&FailedRoutes).await.is_err());
        assert!(delete(&FailedRoutes).await.is_err());
    }
}
