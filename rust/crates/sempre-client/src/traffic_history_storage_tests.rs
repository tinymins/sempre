use chrono::Utc;

use crate::traffic_history::{TrafficDelta, TrafficDimension, TrafficStore};

const MINUTE: i64 = 60_000;

fn delta(label: String) -> TrafficDelta {
    TrafficDelta {
        dimension: TrafficDimension::Host,
        label,
        download: 1,
        upload: 1,
    }
}

fn fill(store: &TrafficStore, minutes: i64) {
    let now = Utc::now().timestamp_millis();
    for minute in 0..minutes {
        let deltas = (0..20)
            .map(|n| delta(format!("host-{minute}-{n}.example")))
            .collect();
        store.record(now - minute * MINUTE, deltas).expect("record");
    }
}

#[test]
fn maintenance_does_not_rewrite_the_file_for_every_sample() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("history.json");
    let store = TrafficStore::open(path.clone()).expect("open");
    let baseline = std::fs::read(&path).ok();
    fill(&store, 2);
    store
        .maintain(Utc::now().timestamp_millis())
        .expect("maintain");
    assert_eq!(std::fs::read(&path).ok(), baseline);
    store.flush().expect("flush");
    assert_ne!(std::fs::read(&path).ok(), baseline);
}

#[test]
fn storage_ceiling_bounds_the_file_and_keeps_recent_records() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("history.json");
    let store = TrafficStore::open_with_ceiling(path.clone(), Some(64 << 10)).expect("open");
    fill(&store, 300);
    store.flush().expect("flush");
    let data = std::fs::read(&path).expect("file");
    assert!(data.len() <= 64 << 10, "{} bytes", data.len());
    // Eviction is proportional, so it must not throw away far more than the excess.
    assert!(data.len() >= 32 << 10, "{} bytes", data.len());
    assert!(String::from_utf8_lossy(&data).contains("host-0-0.example"));
}
