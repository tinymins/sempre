use std::{fs, net::IpAddr, path::Path};

use sempre_converter::Profile;
use serde_json::{Value, json};

use crate::{Plan, TransparentError};

pub(crate) fn prepare(
    core: &str,
    profile: &Profile,
    runtime_config: &Path,
    original_upstreams: Vec<String>,
) -> Result<Plan, TransparentError> {
    if core != "sing-box" {
        return Ok(Plan::default());
    }
    let Some(mut system_dns) = crate::system_dns_intent(profile) else {
        return Ok(Plan::default());
    };
    system_dns.managed_frontend = true;
    system_dns.core_listen_port = match profile.transparent_proxy.tproxy.dns_listen_port {
        0 => 1053,
        port => port,
    };
    system_dns.original_upstreams = original_upstreams;
    let data = fs::read(runtime_config).map_err(|source| TransparentError::Io {
        context: "read desktop DNS frontend runtime configuration".into(),
        source,
    })?;
    let mut config = crate::decode(core, &data)?;
    let plan = Plan {
        core: core.into(),
        system_dns: Some(system_dns),
        ..Plan::default()
    };
    crate::validate_system_dns_config(&plan, &config)?;
    insert_original_dns_bypass(&mut config, &plan)?;
    let mut data =
        serde_json::to_vec_pretty(&config).map_err(|error| TransparentError::Encode {
            core: core.into(),
            detail: error.to_string(),
        })?;
    data.push(b'\n');
    sempre_state::write_atomic(runtime_config, &data, 0o600).map_err(|source| {
        TransparentError::Io {
            context: "write desktop DNS frontend runtime configuration".into(),
            source,
        }
    })?;
    Ok(plan)
}

fn insert_original_dns_bypass(config: &mut Value, plan: &Plan) -> Result<(), TransparentError> {
    let system_dns = plan
        .system_dns
        .as_ref()
        .expect("desktop DNS plan has system DNS");
    let prefixes = system_dns
        .original_upstreams
        .iter()
        .map(|value| {
            value
                .parse::<IpAddr>()
                .map(|address| match address {
                    IpAddr::V4(_) => format!("{address}/32"),
                    IpAddr::V6(_) => format!("{address}/128"),
                })
                .map_err(|_| {
                    TransparentError::Invalid(format!(
                        "desktop original DNS upstream {value:?} is not an IP address"
                    ))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let rules = config
        .pointer_mut("/route/rules")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            TransparentError::Invalid("runtime configuration has no route rules".into())
        })?;
    rules.insert(
        0,
        json!({ "ip_cidr": prefixes, "port": [53], "action": "route", "outbound": "direct" }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn writes_original_dns_bypass_before_global_hijack() {
        let root = tempfile::tempdir().expect("temporary directory");
        let config = root.path().join("config.json");
        fs::write(
            &config,
            serde_json::to_vec(&json!({
                "inbounds": [{
                    "type": "direct", "tag": "sempre-dns-core-in", "listen": "127.0.0.1",
                    "listen_port": 1053, "override_address": "1.1.1.1", "override_port": 53
                }],
                "route": { "rules": [
                    { "inbound": "sempre-dns-core-in", "action": "sniff" },
                    { "inbound": "sempre-dns-core-in", "protocol": "dns", "action": "hijack-dns" },
                    { "protocol": "dns", "action": "hijack-dns" }
                ] }
            }))
            .expect("encode config"),
        )
        .expect("write config");
        let profile: Profile = serde_json::from_value(json!({
            "dns": { "shared": { "systemDnsTakeoverEnabled": true } }
        }))
        .expect("profile");
        let plan = prepare(
            "sing-box",
            &profile,
            &config,
            vec!["223.6.6.6".into(), "2400:3200::1".into()],
        )
        .expect("plan");
        assert_eq!(plan.system_dns.expect("system DNS").core_listen_port, 1053);
        let output: Value =
            serde_json::from_slice(&fs::read(config).expect("read config")).expect("decode config");
        assert_eq!(
            output["route"]["rules"][0],
            json!({
                "ip_cidr": ["223.6.6.6/32", "2400:3200::1/128"],
                "port": [53], "action": "route", "outbound": "direct"
            })
        );
    }
}
