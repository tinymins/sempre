use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use chrono::Utc;
use sempre_core_control::{Connection, ConnectionSnapshot};
use tokio::sync::watch;
use tracing::warn;

use crate::traffic_history::{TrafficDelta, TrafficDimension, TrafficStore};

#[derive(Clone, Copy, Default)]
struct Counters {
    download: i64,
    upload: i64,
}

#[derive(Default)]
struct Accumulator {
    initialized: bool,
    previous: HashMap<String, Counters>,
}

impl Accumulator {
    fn observe(&mut self, snapshot: ConnectionSnapshot) -> Vec<TrafficDelta> {
        let mut next = HashMap::new();
        let mut deltas = Vec::new();
        for connection in snapshot.connections {
            let current = Counters {
                download: connection.download,
                upload: connection.upload,
            };
            let previous = self
                .previous
                .get(&connection.id)
                .copied()
                .unwrap_or_else(|| {
                    if self.initialized {
                        Counters::default()
                    } else {
                        current
                    }
                });
            let download = (current.download - previous.download).max(0);
            let upload = (current.upload - previous.upload).max(0);
            next.insert(connection.id.clone(), current);
            if download > 0 || upload > 0 {
                deltas.extend(connection_deltas(&connection, download, upload));
            }
        }
        self.initialized = true;
        self.previous = next;
        deltas
    }
}

fn connection_deltas(connection: &Connection, download: i64, upload: i64) -> Vec<TrafficDelta> {
    let labels = [
        (
            TrafficDimension::Device,
            value_or_unknown(&connection.metadata.source_ip),
        ),
        (
            TrafficDimension::User,
            value_or_unknown(&connection.metadata.inbound_user),
        ),
        (
            TrafficDimension::Host,
            value_or_unknown(&connection.metadata.host),
        ),
        (
            TrafficDimension::Outbound,
            connection
                .chains
                .first()
                .cloned()
                .unwrap_or_else(|| "direct".into()),
        ),
        (
            TrafficDimension::Process,
            value_or_unknown(&connection.metadata.process),
        ),
    ];
    labels
        .into_iter()
        .map(|(dimension, label)| TrafficDelta {
            dimension,
            label,
            download,
            upload,
        })
        .collect()
}

fn value_or_unknown(value: &str) -> String {
    if value.is_empty() {
        "unknown".into()
    } else {
        value.into()
    }
}

pub(crate) async fn run(
    store: Arc<TrafficStore>,
    control_path: PathBuf,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut accumulator = Accumulator::default();
    let mut flush = tokio::time::interval(Duration::from_secs(5));
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let Ok(client) = sempre_core_control::Client::from_file(&control_path) else {
            if retry_or_shutdown(&store, &mut shutdown).await {
                return;
            }
            continue;
        };
        let connection = tokio::select! {
            result = client.stream("connections") => result,
            result = shutdown.changed() => {
                if result.is_err() || *shutdown.borrow() {
                    flush_shutdown(&store);
                    return;
                }
                continue;
            }
        };
        let Ok(mut stream) = connection else {
            if retry_or_shutdown(&store, &mut shutdown).await {
                return;
            }
            continue;
        };
        loop {
            tokio::select! {
                result = shutdown.changed() => {
                    if result.is_err() || *shutdown.borrow() {
                        flush_shutdown(&store);
                        return;
                    }
                }
                snapshot = stream.next_connections() => match snapshot {
                    Ok(snapshot) => {
                    let deltas = accumulator.observe(snapshot);
                    if let Err(error) = store.record(Utc::now().timestamp_millis(), deltas) {
                        warn!(%error, "failed to record traffic history");
                    }
                    }
                    Err(_) => break,
                },
                _ = flush.tick() => maintain_store(&store),
            }
        }
        if retry_or_shutdown(&store, &mut shutdown).await {
            return;
        }
    }
}

async fn retry_or_shutdown(store: &TrafficStore, shutdown: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        result = shutdown.changed() => {
            if result.is_err() || *shutdown.borrow() {
                flush_shutdown(store);
                true
            } else {
                false
            }
        }
        () = tokio::time::sleep(Duration::from_secs(1)) => {
            maintain_store(store);
            false
        },
    }
}

fn maintain_store(store: &TrafficStore) {
    if let Err(error) = store.maintain(Utc::now().timestamp_millis()) {
        warn!(%error, "failed to rotate or persist traffic history");
    }
}

fn flush_shutdown(store: &TrafficStore) {
    if let Err(error) = store.flush() {
        warn!(%error, "failed to flush traffic history during shutdown");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection(id: &str, download: i64, upload: i64, host: &str) -> Connection {
        Connection {
            id: id.into(),
            download,
            upload,
            chains: vec!["proxy".into()],
            metadata: sempre_core_control::ConnectionMetadata {
                source_ip: "192.0.2.4".into(),
                host: host.into(),
                process: "browser".into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn baselines_existing_connections_and_counts_future_deltas() {
        let mut accumulator = Accumulator::default();
        assert!(
            accumulator
                .observe(ConnectionSnapshot {
                    connections: vec![connection("existing", 100, 20, "example.com")],
                    ..Default::default()
                })
                .is_empty()
        );
        let deltas = accumulator.observe(ConnectionSnapshot {
            connections: vec![
                connection("existing", 140, 25, "example.com"),
                connection("new", 12, 3, "new.example"),
            ],
            ..Default::default()
        });
        let hosts = deltas
            .iter()
            .filter(|delta| delta.dimension == TrafficDimension::Host)
            .map(|delta| (delta.label.as_str(), delta.download, delta.upload))
            .collect::<Vec<_>>();
        assert_eq!(hosts, [("example.com", 40, 5), ("new.example", 12, 3)]);
    }
}
