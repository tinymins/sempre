use super::{ProcessInfo, SocketInfo, command, endpoint};

#[cfg(target_os = "macos")]
pub(super) async fn collect(processes: &[ProcessInfo]) -> Result<Vec<SocketInfo>, String> {
    let pids = processes
        .iter()
        .map(|process| process.pid.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let output = command(
        "/usr/sbin/lsof",
        &["-nP", "-a", "-i", "-FpcPn", "-p", &pids],
    )
    .await?;
    let mut pid = 0;
    let mut protocol = String::new();
    let mut sockets = Vec::new();
    for line in output.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse().unwrap_or(0);
        } else if let Some(value) = line.strip_prefix('P') {
            protocol = value.to_ascii_lowercase();
        } else if let Some(value) = line.strip_prefix('n')
            && let Some((local, remote)) = value.split_once("->")
            && let (Some((local_ip, local_port)), Some((remote_ip, remote_port))) =
                (endpoint(local), endpoint(remote))
            && processes.iter().any(|process| process.pid == pid)
        {
            sockets.push(SocketInfo {
                pid,
                protocol: protocol.clone(),
                local_ip,
                local_port,
                remote_ip,
                remote_port,
            });
        }
    }
    Ok(sockets)
}

#[cfg(target_os = "linux")]
pub(super) async fn collect(processes: &[ProcessInfo]) -> Result<Vec<SocketInfo>, String> {
    let output = command("ss", &["-H", "-n", "-t", "-u", "-a", "-p"]).await?;
    let mut sockets = Vec::new();
    for line in output.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 7 {
            continue;
        }
        let (Some((local_ip, local_port)), Some((remote_ip, remote_port))) =
            (endpoint(fields[4]), endpoint(fields[5]))
        else {
            continue;
        };
        for owner in line.split("pid=").skip(1) {
            let Some(pid) = owner
                .split(',')
                .next()
                .and_then(|pid| pid.parse::<u32>().ok())
            else {
                continue;
            };
            if processes.iter().any(|process| process.pid == pid) {
                sockets.push(SocketInfo {
                    pid,
                    protocol: fields[0].into(),
                    local_ip: local_ip.clone(),
                    local_port,
                    remote_ip: remote_ip.clone(),
                    remote_port,
                });
            }
        }
    }
    Ok(sockets)
}

#[cfg(target_os = "windows")]
pub(super) async fn collect(processes: &[ProcessInfo]) -> Result<Vec<SocketInfo>, String> {
    let output = command("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command",
        "$ErrorActionPreference='Stop'; @(Get-NetTCPConnection | Where-Object {$_.RemotePort -gt 0} | Select-Object OwningProcess,LocalAddress,LocalPort,RemoteAddress,RemotePort) | ConvertTo-Json -Compress"])
        .await?;
    if output.trim().is_empty() {
        return Ok(Vec::new());
    }
    let entries: serde_json::Value = serde_json::from_str(&output)
        .map_err(|_| "Windows socket inventory returned invalid JSON.".to_owned())?;
    let rows = if let Some(rows) = entries.as_array() {
        rows.clone()
    } else if entries.is_null() {
        Vec::new()
    } else {
        vec![entries]
    };
    let mut sockets = Vec::new();
    for row in rows {
        let Some(pid) = row["OwningProcess"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
        else {
            continue;
        };
        if !processes.iter().any(|process| process.pid == pid) {
            continue;
        }
        let local = format!(
            "{}:{}",
            row["LocalAddress"].as_str().unwrap_or_default(),
            row["LocalPort"]
        );
        let remote = format!(
            "{}:{}",
            row["RemoteAddress"].as_str().unwrap_or_default(),
            row["RemotePort"]
        );
        if let (Some((local_ip, local_port)), Some((remote_ip, remote_port))) =
            (endpoint(&local), endpoint(&remote))
        {
            sockets.push(SocketInfo {
                pid,
                protocol: "tcp".into(),
                local_ip,
                local_port,
                remote_ip,
                remote_port,
            });
        }
    }
    Ok(sockets)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(super) async fn collect(_processes: &[ProcessInfo]) -> Result<Vec<SocketInfo>, String> {
    Err("Process socket collection is unsupported on this platform.".into())
}
