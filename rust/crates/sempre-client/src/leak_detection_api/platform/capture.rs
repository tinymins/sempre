use std::{
    fmt::Write,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use serde::Serialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::{Child, Command},
    sync::mpsc,
    task::JoinHandle,
};

#[cfg(any(target_os = "macos", target_os = "linux"))]
use super::command;
use super::endpoint;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PacketInfo {
    pub interface: String,
    pub interface_kind: String,
    pub direction: String,
    pub protocol: String,
    pub source_ip: String,
    pub source_port: u16,
    pub destination_ip: String,
    pub destination_port: u16,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureSnapshot {
    pub status: String,
    pub detail: String,
    pub packets: Vec<PacketInfo>,
}

pub(crate) struct CaptureSession {
    workers: Vec<Worker>,
    receiver: mpsc::Receiver<PacketInfo>,
    dropped: Arc<AtomicUsize>,
    gaps: Vec<String>,
}

struct Worker {
    interface: String,
    child: Child,
    reader: JoinHandle<()>,
    errors: JoinHandle<Vec<u8>>,
    failed: bool,
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        for worker in &mut self.workers {
            let _ = worker.child.start_kill();
            worker.reader.abort();
            worker.errors.abort();
        }
    }
}

pub(crate) async fn start_capture() -> CaptureSession {
    let (sender, receiver) = mpsc::channel(512);
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut session = CaptureSession {
        workers: Vec::new(),
        receiver,
        dropped,
        gaps: Vec::new(),
    };
    let interfaces = match interfaces().await {
        Ok(interfaces) => interfaces,
        Err(error) => {
            session.gaps.push(error);
            return session;
        }
    };
    for (interface, kind) in interfaces {
        match worker(&interface, &kind, sender.clone(), session.dropped.clone()) {
            Ok(worker) => session.workers.push(worker),
            Err(error) => session.gaps.push(error),
        }
    }
    if session.workers.is_empty() {
        session
            .gaps
            .push("No usable default-route interface was found for passive capture.".into());
    }
    session
}

impl CaptureSession {
    pub(crate) fn snapshot(&mut self) -> CaptureSnapshot {
        for worker in &mut self.workers {
            if !worker.failed && !matches!(worker.child.try_wait(), Ok(None)) {
                worker.failed = true;
                self.gaps.push(format!("Capture on {} stopped, reached its 20,000-packet limit, or failed. OS capture permission, tcpdump availability, and outgoing-filter support must be checked; no privilege escalation was attempted.", worker.interface));
            }
        }
        let mut packets = Vec::new();
        while let Ok(packet) = self.receiver.try_recv() {
            packets.push(packet);
        }
        let active: Vec<_> = self
            .workers
            .iter()
            .filter(|worker| !worker.failed)
            .map(|worker| worker.interface.as_str())
            .collect();
        let mut detail = format!(
            "Outgoing header metadata on initial IPv4/IPv6 default-route interfaces [{}]. Policy routes, other interfaces, route changes, kernel drops, unparsed packets and traffic outside this observation window remain uncovered. Device-level observations are not automatically application or gateway-WAN attribution.",
            active.join(", ")
        );
        let dropped = self.dropped.load(Ordering::Relaxed);
        if dropped > 0 {
            let _ = write!(
                detail,
                " {dropped} metadata records were omitted by collection limits or parser coverage."
            );
        }
        if !self.gaps.is_empty() {
            detail.push(' ');
            detail.push_str(&self.gaps.join(" "));
        }
        CaptureSnapshot {
            status: if active.is_empty() {
                "unavailable"
            } else {
                "partial"
            }
            .into(),
            detail,
            packets,
        }
    }
}

fn worker(
    interface: &str,
    kind: &str,
    sender: mpsc::Sender<PacketInfo>,
    dropped: Arc<AtomicUsize>,
) -> Result<Worker, String> {
    let program = if cfg!(target_os = "macos") {
        "/usr/sbin/tcpdump"
    } else {
        "tcpdump"
    };
    let mut child = Command::new(program)
        .args([
            "-i",
            interface,
            "-Q",
            "out",
            "-nn",
            "-q",
            "-t",
            "-l",
            "-p",
            "-s",
            "128",
            "-c",
            "20000",
            "tcp or udp",
        ])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| {
            format!("Cannot start tcpdump on {interface}; tool or capture permission unavailable.")
        })?;
    let stdout = child.stdout.take().ok_or("Capture stdout unavailable.")?;
    let stderr = child.stderr.take().ok_or("Capture stderr unavailable.")?;
    let name = interface.to_owned();
    let kind = kind.to_owned();
    let reader = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(packet) = packet(&line, &name, &kind) {
                if sender.try_send(packet).is_err() {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    let errors = tokio::spawn(async move {
        let mut bytes = Vec::new();
        let _ = stderr.take(8192).read_to_end(&mut bytes).await;
        bytes
    });
    Ok(Worker {
        interface: interface.into(),
        child,
        reader,
        errors,
        failed: false,
    })
}

fn packet(line: &str, interface: &str, kind: &str) -> Option<PacketInfo> {
    let fields: Vec<_> = line.split_whitespace().collect();
    if fields.len() < 5 || !matches!(fields[0], "IP" | "IP6") || fields[2] != ">" {
        return None;
    }
    let parse = |value: &str| {
        let (ip, port) = value.trim_end_matches(':').rsplit_once('.')?;
        endpoint(&format!("[{ip}]:{port}"))
    };
    let (source_ip, source_port) = parse(fields[1])?;
    let (destination_ip, destination_port) = parse(fields[3])?;
    let protocol = fields[4].trim_end_matches(',').to_ascii_lowercase();
    if !matches!(protocol.as_str(), "tcp" | "udp") {
        return None;
    }
    Some(PacketInfo {
        interface: interface.into(),
        interface_kind: kind.into(),
        direction: "out".into(),
        protocol,
        source_ip,
        source_port,
        destination_ip,
        destination_port,
    })
}

#[cfg(target_os = "macos")]
async fn interfaces() -> Result<Vec<(String, String)>, String> {
    let hardware = command("/usr/sbin/networksetup", &["-listallhardwareports"])
        .await
        .unwrap_or_default();
    let mut kinds = std::collections::HashMap::new();
    let mut port = "";
    for line in hardware.lines() {
        if let Some(value) = line.strip_prefix("Hardware Port: ") {
            port = value;
        }
        if let Some(device) = line.strip_prefix("Device: ") {
            let kind = if port.contains("Wi-Fi") || port.contains("AirPort") {
                "wifi"
            } else if port.contains("Ethernet") || port.contains("LAN") {
                "ethernet"
            } else {
                "unknown"
            };
            kinds.insert(device.to_owned(), kind.to_owned());
        }
    }
    let mut interfaces = Vec::new();
    for args in [
        vec!["-n", "get", "-inet", "default"],
        vec!["-n", "get", "-inet6", "default"],
    ] {
        if let Ok(output) = command("/sbin/route", &args).await
            && let Some(name) = output
                .lines()
                .find_map(|line| line.trim().strip_prefix("interface: "))
            && !interfaces.iter().any(|(existing, _)| existing == name)
        {
            interfaces.push((
                name.to_owned(),
                kinds.get(name).cloned().unwrap_or_else(|| "unknown".into()),
            ));
        }
    }
    Ok(interfaces)
}

#[cfg(target_os = "linux")]
async fn interfaces() -> Result<Vec<(String, String)>, String> {
    let mut interfaces = Vec::new();
    for family in ["-4", "-6"] {
        let Ok(output) = command("ip", &["-j", family, "route", "show", "default"]).await else {
            continue;
        };
        let rows: Vec<serde_json::Value> = serde_json::from_str(&output)
            .map_err(|_| "Cannot parse default-route inventory.".to_owned())?;
        for row in rows {
            if let Some(name) = row["dev"].as_str()
                && !interfaces.iter().any(|(existing, _)| existing == name)
            {
                // Physical device linkage excludes TUN and bridge interfaces; no route-only inference.
                let path = std::path::Path::new("/sys/class/net").join(name);
                let kind = if path.join("wireless").exists() {
                    "wifi"
                } else if path.join("device").exists() {
                    "ethernet"
                } else {
                    "unknown"
                };
                interfaces.push((name.to_owned(), kind.into()));
            }
        }
    }
    Ok(interfaces)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
async fn interfaces() -> Result<Vec<(String, String)>, String> {
    Err("Scoped passive packet capture is unavailable on this platform; no global packet-capture session was started or modified.".into())
}
