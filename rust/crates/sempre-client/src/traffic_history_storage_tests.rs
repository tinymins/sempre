use chrono::Utc;

use crate::traffic_history::{TrafficDelta, TrafficDimension, TrafficStore, capped};
use crate::traffic_rotation::TrafficSettings;

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
    let path = root.path().join("history");
    let store = TrafficStore::open(path.clone()).expect("open");
    let cursor = path.join("cursor");
    let baseline = std::fs::read(&cursor).ok();
    fill(&store, 2);
    store
        .maintain(Utc::now().timestamp_millis())
        .expect("maintain");
    assert_eq!(std::fs::read(&cursor).ok(), baseline);
    store.flush().expect("flush");
    assert_ne!(std::fs::read(&cursor).ok(), baseline);
}

#[test]
fn storage_ceiling_bounds_the_file_and_keeps_recent_records() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("history");
    let limit = 64 << 10;
    let store = TrafficStore::open_with_ceiling(path.clone(), Some(limit)).expect("open");
    fill(&store, 300);
    store.flush().expect("flush");
    let bytes: u64 = std::fs::read_dir(&path)
        .unwrap()
        .map(|file| file.unwrap().metadata().unwrap().len())
        .sum();
    assert!(bytes <= limit, "{bytes} bytes");
    assert!(bytes >= 32 << 10, "{bytes} bytes");
    drop(store);
    let reopened = TrafficStore::open_with_ceiling(path, Some(limit)).unwrap();
    let history = reopened
        .history(0, TrafficDimension::Host, Utc::now().timestamp_millis())
        .unwrap();
    let data = serde_json::to_value(history).unwrap();
    let totals = data["totals"].as_array().unwrap();
    for n in 0..20 {
        let row = totals
            .iter()
            .find(|row| row["label"] == format!("host-0-{n}.example"))
            .expect("latest record");
        assert_eq!(row["download"], 1);
        assert_eq!(row["upload"], 1);
    }
}

#[test]
fn reported_limit_never_exceeds_the_storage_ceiling() {
    let ceiling = 4 << 20;
    let limited = |max_bytes| TrafficSettings {
        max_bytes,
        ..TrafficSettings::default()
    };
    assert_eq!(
        capped(limited(Some(32 << 20)), Some(ceiling)).max_bytes,
        Some(ceiling)
    );
    assert_eq!(
        capped(limited(None), Some(ceiling)).max_bytes,
        Some(ceiling)
    );
    assert_eq!(
        capped(limited(Some(1 << 20)), Some(ceiling)).max_bytes,
        Some(1 << 20)
    );
    assert_eq!(capped(limited(None), None).max_bytes, None);
}
