#![cfg(unix)]

use std::{fs, sync::Arc, time::Duration};

use sempre_state::{Layout, Store as StateStore};
use tokio::{sync::watch, time::sleep};

use crate::{Config, Controller};

#[cfg(unix)]
#[tokio::test]
async fn supervises_independent_tunnel_instances() {
    use std::os::unix::fs::PermissionsExt as _;

    let root = tempfile::tempdir().expect("temporary directory");
    let layout = Layout::at(root.path());
    StateStore::new(layout.clone())
        .initialize()
        .expect("layout");
    let binary = crate::package::binary_path(&layout);
    fs::create_dir_all(binary.parent().expect("binary parent")).expect("tool directory");
    fs::write(
        &binary,
        "#!/bin/sh\ntrap 'exit 0' TERM INT\nwhile :; do sleep 1; done\n",
    )
    .expect("fake binary");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).expect("executable");
    let controller = Arc::new(Controller::new(layout).expect("controller"));
    let config: Config = serde_json::from_value(serde_json::json!({
        "schema": 1, "instances": [{
            "id": "hz", "name": "Hangzhou", "desired_state": "running",
            "server_url": "wss://hz.example.com", "dns_resolvers": [],
            "prefer_ipv4": false, "websocket_ping": "15s",
            "connection_retry_max_backoff": "30s", "upgrade_path_prefix": "",
            "forwards": [{
                "id": "hz-wg", "name": "WG", "listen_port": 52001,
                "remote_host": "127.0.0.1", "remote_port": 31088,
                "timeout_seconds": 0
            }]
        }]
    }))
    .expect("config");
    controller.update(config).await.expect("update");
    let (shutdown, receiver) = watch::channel(false);
    let running = tokio::spawn(Arc::clone(&controller).run(receiver));
    wait_for_state(&controller, "hz", "running").await;
    controller.action("hz", "stop").await.expect("stop");
    wait_for_state(&controller, "hz", "stopped").await;
    shutdown.send(true).expect("shutdown");
    running.await.expect("task").expect("controller run");
}

async fn wait_for_state(controller: &Controller, id: &str, state: &str) {
    for _ in 0..100 {
        let status = controller.status().expect("status");
        if status
            .instances
            .iter()
            .any(|item| item.id == id && item.state == state)
        {
            return;
        }
        sleep(Duration::from_millis(25)).await;
    }
    panic!("tunnel instance {id} did not reach {state}");
}
