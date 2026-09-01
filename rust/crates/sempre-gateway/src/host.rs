use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
};

use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt as _, process::Command, time::timeout};

use crate::{Config, GatewayError};

#[derive(Clone, Debug, Deserialize)]
pub struct HostPlanRequest {
    pub config: Config,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HostApplyRequest {
    pub config: Config,
    pub confirm: bool,
    #[serde(default)]
    pub private_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HostPlan {
    pub topology: String,
    pub summary: String,
    pub warnings: Vec<String>,
    pub commands: Vec<String>,
    pub persistent_commands: Vec<String>,
    pub apply_by_ssh: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub output: Vec<String>,
}

pub fn build_host_plan(mut config: Config) -> Result<HostPlan, GatewayError> {
    config.normalize();
    config.validate()?;
    let interface = fallback(&config.lan.interface, "<lan-interface>");
    let wan = fallback(&config.lan.wan_interface, "<wan-interface>");
    let gateway = config
        .lan
        .gateway_cidr
        .split_once('/')
        .map_or(config.lan.gateway_cidr.as_str(), |(address, _)| address);
    let mut commands = vec![
        format!(
            "ip addr replace {} dev {interface}",
            config.lan.gateway_cidr
        ),
        format!("ip link set {interface} up"),
        "sysctl -w net.ipv4.ip_forward=1".into(),
    ];
    let mut persistent_commands =
        vec!["printf 'net.ipv4.ip_forward=1\\n' >/etc/sysctl.d/99-sempre-gateway.conf".into()];
    let mut warnings = Vec::new();
    if config.lan.nat_enabled {
        commands.extend([
            "nft add table inet sempre_gateway_pve 2>/dev/null || true".into(),
            "nft 'add chain inet sempre_gateway_pve postrouting { type nat hook postrouting priority srcnat; policy accept; }' 2>/dev/null || true".into(),
            format!(
                "nft add rule inet sempre_gateway_pve postrouting oifname {wan:?} ip saddr {} masquerade",
                masked_prefix(&config.lan.gateway_cidr)?
            ),
        ]);
        persistent_commands.push(
            "# Persist nftables according to the host policy, for example via /etc/nftables.conf."
                .into(),
        );
    } else {
        warnings.push("NAT is disabled; upstream routing must already know the LAN prefix.".into());
    }
    if config.dhcp.enabled {
        warnings.push(format!(
            "VMs should use {gateway} as default gateway and DNS server."
        ));
    }
    Ok(HostPlan {
        topology: config.topology.clone(),
        summary: format!(
            "Prepare {} with gateway {} on {interface}",
            config.topology, config.lan.gateway_cidr
        ),
        warnings,
        commands,
        persistent_commands,
        apply_by_ssh: config.topology == "remote-pve",
        output: Vec::new(),
    })
}

pub async fn apply_host_plan(request: HostApplyRequest) -> Result<HostPlan, GatewayError> {
    let mut plan = build_host_plan(request.config.clone())?;
    if !request.confirm {
        return Err(GatewayError::invalid("host apply requires confirmation"));
    }
    if request.config.lan.interface.is_empty() {
        return Err(GatewayError::invalid(
            "LAN interface is required before applying a host plan",
        ));
    }
    if request.config.lan.nat_enabled && request.config.lan.wan_interface.is_empty() {
        return Err(GatewayError::invalid(
            "WAN interface is required when NAT is enabled",
        ));
    }
    let mut commands = plan.commands.clone();
    if request.config.pve.apply_persistent {
        commands.extend(plan.persistent_commands.clone());
    }
    plan.output = if request.config.topology == "remote-pve" {
        run_ssh_commands(&request, &commands).await?
    } else {
        run_local_commands(&commands).await?
    };
    Ok(plan)
}

async fn run_local_commands(commands: &[String]) -> Result<Vec<String>, GatewayError> {
    let mut output = Vec::new();
    for command in commands {
        let result = run_command("sh", &["-c", command], None).await?;
        output.push(command_output(command, &result.stdout, &result.stderr));
        if !result.status.success() {
            return Err(GatewayError::invalid(format!(
                "run {command:?}: {}",
                result.status
            )));
        }
    }
    Ok(output)
}

async fn run_ssh_commands(
    request: &HostApplyRequest,
    commands: &[String],
) -> Result<Vec<String>, GatewayError> {
    let pve = &request.config.pve;
    if pve.host.trim().is_empty() {
        return Err(GatewayError::invalid("PVE host is required for SSH apply"));
    }
    let temporary = tempfile::tempdir()
        .map_err(|error| GatewayError::io("create SSH temporary directory", error))?;
    let key = prepare_key(temporary.path(), &request.private_key, &pve.key_path)?;
    let known_hosts =
        prepare_known_host(temporary.path(), &pve.host, pve.port, &pve.fingerprint).await?;
    let target = format!("{}@{}", fallback(&pve.user, "root"), pve.host);
    let mut common = vec![
        "-i".to_owned(),
        key.to_string_lossy().into_owned(),
        "-p".to_owned(),
        pve.port.to_string(),
        "-o".to_owned(),
        "BatchMode=yes".into(),
        "-o".to_owned(),
        "ConnectTimeout=10".into(),
    ];
    if let Some(known_hosts) = known_hosts {
        common.extend([
            "-o".into(),
            "StrictHostKeyChecking=yes".into(),
            "-o".into(),
            format!("UserKnownHostsFile={}", known_hosts.display()),
        ]);
    } else {
        common.extend([
            "-o".into(),
            "StrictHostKeyChecking=no".into(),
            "-o".into(),
            "UserKnownHostsFile=/dev/null".into(),
        ]);
    }
    let mut output = Vec::new();
    for command in commands {
        let mut arguments = common.clone();
        arguments.extend([target.clone(), command.clone()]);
        let refs: Vec<_> = arguments.iter().map(String::as_str).collect();
        let result = run_command("ssh", &refs, None).await?;
        output.push(command_output(command, &result.stdout, &result.stderr));
        if !result.status.success() {
            return Err(GatewayError::invalid(format!(
                "run remote {command:?}: {}",
                result.status
            )));
        }
    }
    Ok(output)
}

fn prepare_key(directory: &Path, inline: &str, configured: &str) -> Result<PathBuf, GatewayError> {
    if !inline.trim().is_empty() {
        let path = directory.join("id_sempre");
        fs::write(&path, inline.trim().as_bytes())
            .map_err(|error| GatewayError::io("write temporary SSH key", error))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .map_err(|error| GatewayError::io("secure temporary SSH key", error))?;
        }
        return Ok(path);
    }
    if configured.trim().is_empty() {
        Err(GatewayError::invalid("SSH private key is required"))
    } else {
        Ok(configured.into())
    }
}

async fn prepare_known_host(
    directory: &Path,
    host: &str,
    port: u16,
    expected: &str,
) -> Result<Option<PathBuf>, GatewayError> {
    if expected.trim().is_empty() {
        return Ok(None);
    }
    let scan = run_command("ssh-keyscan", &["-p", &port.to_string(), host], None).await?;
    if !scan.status.success() || scan.stdout.is_empty() {
        return Err(GatewayError::invalid("scan PVE host key failed"));
    }
    let fingerprint = run_command(
        "ssh-keygen",
        &["-lf", "-", "-E", "sha256"],
        Some(&scan.stdout),
    )
    .await?;
    let fingerprint = String::from_utf8_lossy(&fingerprint.stdout);
    if !fingerprint
        .split_whitespace()
        .any(|value| value == expected.trim())
    {
        return Err(GatewayError::invalid(format!(
            "PVE host key fingerprint mismatch: expected {}",
            expected.trim()
        )));
    }
    let path = directory.join("known_hosts");
    fs::write(&path, scan.stdout)
        .map_err(|error| GatewayError::io("write temporary known_hosts", error))?;
    Ok(Some(path))
}

async fn run_command(
    program: &str,
    arguments: &[&str],
    input: Option<&[u8]>,
) -> Result<std::process::Output, GatewayError> {
    let mut command = Command::new(program);
    command.args(arguments).kill_on_drop(true);
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| GatewayError::io(format!("start {program}"), error))?;
    if let Some(input) = input {
        child
            .stdin
            .take()
            .expect("piped stdin")
            .write_all(input)
            .await
            .map_err(|error| GatewayError::io(format!("write {program} input"), error))?;
    }
    timeout(std::time::Duration::from_secs(30), child.wait_with_output())
        .await
        .map_err(|_| GatewayError::invalid(format!("{program} timed out")))?
        .map_err(|error| GatewayError::io(format!("wait for {program}"), error))
}

fn command_output(command: &str, stdout: &[u8], stderr: &[u8]) -> String {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    );
    let text = text.trim();
    if text.is_empty() {
        format!("$ {command}")
    } else {
        format!("$ {command}\n{text}")
    }
}

fn fallback<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value
    }
}

fn masked_prefix(value: &str) -> Result<String, GatewayError> {
    let (address, prefix) = value
        .split_once('/')
        .ok_or_else(|| GatewayError::invalid("invalid gateway CIDR"))?;
    let address: std::net::Ipv4Addr = address
        .parse()
        .map_err(|_| GatewayError::invalid("invalid gateway CIDR"))?;
    let prefix: u8 = prefix
        .parse()
        .map_err(|_| GatewayError::invalid("invalid gateway CIDR"))?;
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    Ok(format!(
        "{}/{prefix}",
        std::net::Ipv4Addr::from(u32::from(address) & mask)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_plan_matches_gateway_policy_and_apply_rejects_placeholders() {
        let plan = build_host_plan(Config::default()).expect("plan");
        assert!(
            plan.commands
                .iter()
                .any(|command| command.contains("ip_forward=1"))
        );
        assert!(plan.summary.contains("<lan-interface>"));
    }

    #[tokio::test]
    async fn apply_requires_concrete_interfaces_before_running_commands() {
        let error = apply_host_plan(HostApplyRequest {
            config: Config::default(),
            confirm: true,
            private_key: String::new(),
        })
        .await
        .expect_err("missing interface");
        assert!(error.to_string().contains("LAN interface is required"));
    }
}
