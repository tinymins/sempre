use std::{fs, path::Path};

use serde_json::Value;

use super::{CaptureSettings, Offload};
use crate::{TransparentError, command};

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'-'))
}

pub(super) fn validate_names(values: &[String]) -> Result<(), TransparentError> {
    match values.iter().find(|value| !valid_name(value)) {
        Some(value) => Err(TransparentError::Invalid(format!(
            "invalid interface name {value:?}"
        ))),
        None => Ok(()),
    }
}

pub(super) fn validate_macs(values: &[String]) -> Result<(), TransparentError> {
    let valid = |value: &String| {
        let parts = value.split(':').collect::<Vec<_>>();
        parts.len() == 6
            && parts
                .iter()
                .all(|part| part.len() == 2 && part.bytes().all(|b| b.is_ascii_hexdigit()))
    };
    match values.iter().find(|value| !valid(value)) {
        Some(value) => Err(TransparentError::Invalid(format!(
            "invalid bypass MAC {value:?}"
        ))),
        None => Ok(()),
    }
}

/// `(interface, address/length)` for every global address on a bridge.
pub(super) fn bridge_addresses(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let name = *fields.get(1)?;
            let address = *fields.get(3)?;
            name.starts_with("br")
                .then(|| (name.to_owned(), address.to_owned()))
        })
        .collect()
}

pub(super) fn lan_interfaces(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let name = line.split(": ").nth(1)?;
            let name = name.split('@').next()?;
            (name.starts_with("br") || name == "wireguard").then(|| name.to_owned())
        })
        .collect()
}

pub(super) async fn resolve_lan(
    runner: &dyn command::Runner,
    settings: &CaptureSettings,
) -> Result<Vec<String>, TransparentError> {
    if !settings.lan_interfaces.is_empty() {
        return Ok(settings.lan_interfaces.clone());
    }
    let output =
        command::require_success("ip", runner.run("ip", &["-o", "link", "show"], None).await?)?;
    Ok(lan_interfaces(&output.stdout))
}

/// Physical devices beneath a bridge or VLAN. Flowtables attach to these only.
fn lower_devices(sys: &Path, device: &str, result: &mut Vec<String>) {
    let directory = sys.join(device);
    let lower = fs::read_dir(&directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_prefix("lower_"))
                .map(str::to_owned)
        })
        .collect::<Vec<_>>();
    for name in &lower {
        lower_devices(sys, name, result);
    }
    let ethernet = fs::read_to_string(directory.join("type")).is_ok_and(|kind| kind.trim() == "1");
    if lower.is_empty() && ethernet && !result.iter().any(|existing| existing == device) {
        result.push(device.to_owned());
    }
}

/// Offload skips per-packet policy, so any shaping fails closed to the normal path.
pub(super) fn traffic_control_active(qdiscs: &str) -> bool {
    qdiscs.lines().any(|line| {
        let mut fields = line.split_whitespace();
        fields.next() == Some("qdisc")
            && !matches!(
                fields.next(),
                Some("noqueue" | "fq_codel" | "mq" | "pfifo_fast")
            )
    })
}

async fn shaping_enabled(runner: &dyn command::Runner) -> bool {
    let Ok(output) = runner.run("tc", &["qdisc", "show"], None).await else {
        return true;
    };
    if !output.success || traffic_control_active(&output.stdout) {
        return true;
    }
    for package in ["sqm", "qos"] {
        if let Ok(output) = runner.run("uci", &["-q", "show", package], None).await
            && output
                .stdout
                .lines()
                .any(|line| line.ends_with(".enabled='1'"))
        {
            return true;
        }
    }
    false
}

fn json_output(output: Result<command::Output, TransparentError>) -> Value {
    output
        .ok()
        .filter(|output| output.success)
        .and_then(|output| serde_json::from_str(&output.stdout).ok())
        .unwrap_or(Value::Null)
}

async fn wan_devices(runner: &dyn command::Runner, settings: &CaptureSettings) -> Vec<String> {
    if !settings.offload_wan_interfaces.is_empty() {
        return settings.offload_wan_interfaces.clone();
    }
    let status = json_output(
        runner
            .run("ubus", &["call", "network.interface.wan", "status"], None)
            .await,
    );
    status["l3_device"]
        .as_str()
        .map(|device| vec![device.to_owned()])
        .unwrap_or_default()
}

/// Decides whether selective offload can run, and says why when it cannot.
pub(super) async fn offload(
    runner: &dyn command::Runner,
    sys: &Path,
    settings: &CaptureSettings,
    lan: &[String],
) -> (Option<Offload>, &'static str) {
    if !settings.software_offload {
        return (None, "disabled-by-settings");
    }
    let bridges = lan
        .iter()
        .filter(|device| sys.join(device).join("bridge").is_dir())
        .cloned()
        .collect::<Vec<_>>();
    let wan = wan_devices(runner, settings).await;
    if bridges.is_empty() || wan.is_empty() {
        return (None, "no-eligible-interfaces");
    }
    let dump = json_output(
        runner
            .run("ubus", &["call", "network.interface", "dump"], None)
            .await,
    );
    let underlay = dump["interface"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|interface| {
            interface["l3_device"]
                .as_str()
                .is_some_and(|device| wan.iter().any(|name| name == device))
        })
        .filter_map(|interface| interface["device"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    let all = bridges.iter().chain(&wan).chain(&underlay);
    let mut devices = Vec::new();
    for device in all {
        if !valid_name(device) || !sys.join(device).exists() {
            return (None, "no-eligible-interfaces");
        }
        lower_devices(sys, device, &mut devices);
    }
    if devices.is_empty() {
        return (None, "no-eligible-interfaces");
    }
    if shaping_enabled(runner).await {
        return (None, "traffic-control");
    }
    devices.sort();
    let offload = Offload {
        devices,
        lan: bridges,
        wan,
    };
    (Some(offload), "selective-ipv4-tcp")
}
