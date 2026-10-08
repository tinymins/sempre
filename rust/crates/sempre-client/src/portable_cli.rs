use std::{path::PathBuf, time::Duration};

use reqwest::Client;
use sempre_control::PublicEndpoint;
use sempre_state::{Layout, Mode, set_portable_marker};

use crate::{ClientError, args::Arguments, daemon, diagnostics_cli};

const READY_TIMEOUT: Duration = Duration::from_secs(20);
const READY_POLL: Duration = Duration::from_millis(200);

pub(crate) fn resolve_mode(arguments: &Arguments) -> Result<Mode, ClientError> {
    if arguments.portable {
        Ok(Mode::Portable)
    } else if arguments.system {
        Ok(Mode::System)
    } else {
        Ok(Mode::current()?)
    }
}

pub(crate) fn set_marker(enabled: bool) -> Result<(), ClientError> {
    let executable = std::env::current_exe().map_err(|source| ClientError::Io {
        operation: "locate Sempre executable",
        path: PathBuf::from("sempre"),
        source,
    })?;
    let path = set_portable_marker(&executable, enabled)?;
    let state = if enabled { "enabled" } else { "disabled" };
    println!("Portable marker {state} at {}.", path.display());
    Ok(())
}

pub(crate) async fn run(mode: Mode) -> Result<(), ClientError> {
    if mode != Mode::Portable {
        return Err(ClientError::Runtime(
            "portable run requires --portable mode or an enabled portable marker".into(),
        ));
    }
    let layout = Layout::for_mode(mode)?;
    let watcher_layout = layout.clone();
    tokio::spawn(async move {
        if let Err(error) = announce_when_ready(&watcher_layout).await {
            eprintln!("ERROR: {error}");
        }
    });
    println!("Starting portable Sempre. Press Ctrl+C to stop.");
    daemon::run_with_layout(layout, None, None).await
}

async fn announce_when_ready(layout: &Layout) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("build portable readiness client: {error}"))?;
    let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
    loop {
        if let Ok(endpoint) = PublicEndpoint::read(&layout.endpoint)
            && endpoint_ready(&client, &endpoint.local_url).await
        {
            if ui_ready(&client, &endpoint.local_url).await {
                println!("Portable Web UI: {}", endpoint.local_url);
                diagnostics_cli::open(Mode::Portable).map_err(|error| error.to_string())?;
            } else {
                println!("Portable service: {}", endpoint.local_url);
                println!("Portable Web UI: not installed");
            }
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("portable Web UI did not become ready".into());
        }
        tokio::time::sleep(READY_POLL).await;
    }
}

async fn endpoint_ready(client: &Client, base_url: &str) -> bool {
    request_success(client, &format!("{base_url}/api/v1/health")).await
}

async fn ui_ready(client: &Client, base_url: &str) -> bool {
    request_success(client, &format!("{base_url}/")).await
}

async fn request_success(client: &Client, url: &str) -> bool {
    client
        .get(url)
        .send()
        .await
        .is_ok_and(|response| response.status().is_success())
}
