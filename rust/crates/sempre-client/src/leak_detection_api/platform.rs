use std::{collections::BTreeMap, net::IpAddr, process::Stdio, time::Duration};

use serde::Serialize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tokio::{io::AsyncReadExt, process::Command};

mod capture;
mod sockets;
pub(super) use capture::{CaptureSnapshot, PacketInfo, start_capture};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PlatformSnapshot {
    pub platform: String,
    pub processes: Vec<ProcessInfo>,
    pub sockets: Vec<SocketInfo>,
    pub dns: Vec<DnsObservation>,
    pub gaps: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProcessInfo {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub started_at: u64,
    pub name: String,
    pub app: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SocketInfo {
    pub pid: u32,
    pub protocol: String,
    pub local_ip: String,
    pub local_port: u16,
    pub remote_ip: String,
    pub remote_port: u16,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DnsObservation {
    pub scope: String,
    pub protocol: String,
    pub server: String,
    pub evidence: String,
}

pub(super) async fn collect(target: &str) -> PlatformSnapshot {
    collect_with_probes(target, &[]).await
}

pub(super) async fn collect_with_probes(target: &str, probes: &[ProcessInfo]) -> PlatformSnapshot {
    let target = target.to_owned();
    let probes = probes.to_vec();
    let mut gaps = vec![
        "Socket snapshots can miss short-lived connections and processes hidden by OS permissions. Independent MCP processes are not automatically attributed.".into(),
        "Shared OS DNS resolvers cannot be attributed to an application from socket ownership. DoH on port 443 cannot be identified by ports alone.".into(),
        "Claude Desktop is excluded. Custom Claude Code executable locations or inaccessible executable paths may not be recognized.".into(),
    ];
    let processes = if let Ok((processes, truncated)) =
        tokio::task::spawn_blocking(move || processes(&target, &probes)).await
    {
        if truncated {
            gaps.push("Process inventory reached its 256-process limit.".into());
        }
        processes
    } else {
        gaps.push("Process inventory could not be collected.".into());
        Vec::new()
    };
    let sockets = if processes.is_empty() {
        Vec::new()
    } else {
        match sockets::collect(&processes).await {
            Ok(sockets) => sockets,
            Err(error) => {
                gaps.push(error);
                Vec::new()
            }
        }
    };
    let dns = sockets.iter().filter(|socket| matches!(socket.remote_port, 53 | 853))
        .map(|socket| DnsObservation {
            scope: format!("process:{}", socket.pid),
            protocol: format!("{}/{} candidate", socket.protocol, socket.remote_port),
            server: socket.remote_ip.clone(),
            evidence: "Connected socket to a DNS-associated port; this does not prove a DNS query, encryption, physical egress, or a leak.".into(),
        }).collect();
    if cfg!(target_os = "windows") {
        gaps.push("Windows UDP endpoints do not expose their remote peer; UDP application attribution is not covered by this inventory.".into());
    }
    PlatformSnapshot {
        platform: std::env::consts::OS.into(),
        processes,
        sockets,
        dns,
        gaps,
    }
}

fn processes(target: &str, probes: &[ProcessInfo]) -> (Vec<ProcessInfo>, bool) {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_exe(UpdateKind::OnlyIfNotSet),
    );
    let mut selected = BTreeMap::new();
    for (pid, process) in system.processes() {
        if let Some(app) = app_name(process)
            && (target == "all" || target == app)
        {
            selected.insert(pid.as_u32(), app.to_owned());
        }
    }
    loop {
        let previous = selected.len();
        for (pid, process) in system.processes() {
            if let Some(app) = process
                .parent()
                .and_then(|parent| selected.get(&parent.as_u32()))
                .cloned()
            {
                selected.entry(pid.as_u32()).or_insert(app);
            }
        }
        if previous == selected.len() {
            break;
        }
    }
    for probe in probes {
        if system
            .process(sysinfo::Pid::from_u32(probe.pid))
            .is_some_and(|process| process.start_time() == probe.started_at)
        {
            selected.insert(probe.pid, "probe".into());
        }
    }
    let truncated = selected.len() > 256;
    let processes = selected
        .into_iter()
        .take(256)
        .filter_map(|(pid, app)| {
            let process = system.process(sysinfo::Pid::from_u32(pid))?;
            Some(ProcessInfo {
                pid,
                parent_pid: process.parent().map(sysinfo::Pid::as_u32),
                started_at: process.start_time(),
                name: process.name().to_string_lossy().chars().take(128).collect(),
                app,
            })
        })
        .collect();
    (processes, truncated)
}

fn app_name(process: &sysinfo::Process) -> Option<&'static str> {
    let original_name = process.name().to_string_lossy();
    let name = original_name.to_ascii_lowercase();
    let stem = name.trim_end_matches(".exe");
    if stem == "codex" {
        return Some("codex");
    }
    if stem == "claude" {
        let executable = process.exe().map(|path| {
            path.to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase()
        });
        if executable
            .as_ref()
            .is_some_and(|path| path.contains(".app/contents/"))
        {
            return None;
        }
        // Windows desktop and CLI share an executable name; require a CLI install path.
        if cfg!(target_os = "windows") {
            return executable
                .as_ref()
                .is_some_and(|path| {
                    path.ends_with("/.local/bin/claude.exe")
                        || path.contains("/.local/share/claude/versions/")
                        || path.contains("/@anthropic-ai/claude-code/")
                })
                .then_some("claude");
        }
        if executable.is_none() && original_name != "claude" {
            return None;
        }
        return Some("claude");
    }
    if !matches!(stem, "node" | "bun" | "nodejs") {
        return None;
    }
    // Inspect only script paths, never arguments containing prompts or credentials.
    for argument in process.cmd().iter().take(3).skip(1) {
        let argument = argument.to_string_lossy().replace('\\', "/");
        if argument.contains("/@anthropic-ai/claude-code/") {
            return Some("claude");
        }
        if argument.contains("/@openai/codex/") {
            return Some("codex");
        }
    }
    None
}

async fn command(program: &str, arguments: &[&str]) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(arguments)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| format!("{program} is unavailable or cannot be executed."))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Command output is unavailable.")?;
    let operation = async {
        let mut bytes = Vec::new();
        stdout
            .take(2_097_153)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "Cannot read socket inventory.".to_owned())?;
        if bytes.len() > 2_097_152 {
            return Err("Socket inventory exceeded its output limit.".into());
        }
        let status = child
            .wait()
            .await
            .map_err(|_| "Cannot wait for socket inventory.".to_owned())?;
        let empty_lsof = program.ends_with("lsof") && status.code() == Some(1) && bytes.is_empty();
        if !(status.success() || empty_lsof) {
            return Err(format!(
                "{program} failed; inspect OS permissions and tool availability."
            ));
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    };
    tokio::time::timeout(Duration::from_secs(5), operation)
        .await
        .map_err(|_| format!("{program} exceeded the 5-second limit."))?
}

fn endpoint(value: &str) -> Option<(String, u16)> {
    let (ip, port) = value.rsplit_once(':')?;
    let ip = ip.trim_matches(['[', ']']).split('%').next()?;
    let ip: IpAddr = ip.parse().ok()?;
    let port = port.parse().ok()?;
    (!ip.is_unspecified() && port != 0).then(|| (ip.to_string(), port))
}

pub(super) fn probe_process(pid: u32) -> Option<ProcessInfo> {
    let mut system = System::new();
    let native_pid = sysinfo::Pid::from_u32(pid);
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[native_pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    let process = system.process(native_pid)?;
    Some(ProcessInfo {
        pid,
        parent_pid: Some(std::process::id()),
        started_at: process.start_time(),
        name: "sempre leak-probe".into(),
        app: "probe".into(),
    })
}
