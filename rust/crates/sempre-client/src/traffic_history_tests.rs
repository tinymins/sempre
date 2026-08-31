use super::*;

#[test]
fn startup_migrates_v1_settings_once() {
    let root = tempfile::tempdir().expect("temporary directory");
    let path = root.path().join("traffic.json");
    fs::write(
        &path,
        br#"{"schema":1,"settings":{"retention_hours":72,"max_bytes":33554432},"records":[]}"#,
    )
    .expect("v1 traffic history");

    TrafficStore::open(path.clone()).expect("migrate traffic history");
    let first = fs::read(&path).expect("migrated traffic history");
    let document: serde_json::Value = serde_json::from_slice(&first).expect("migrated JSON");
    assert_eq!(document["schema"], 2);
    assert_eq!(document["settings"]["reset_day"], serde_json::Value::Null);
    assert_eq!(document["settings"]["retention_months"], 12);
    assert_eq!(
        document["applied_migrations"][0]["id"],
        "v0002_monthly_retention"
    );

    TrafficStore::open(path.clone()).expect("reopen current history");
    assert_eq!(fs::read(path).expect("unchanged traffic history"), first);
}

#[test]
fn invalid_v1_history_does_not_advance_the_migration_ledger() {
    let root = tempfile::tempdir().expect("temporary directory");
    let path = root.path().join("traffic.json");
    let fixture =
        br#"{"schema":1,"settings":{"retention_hours":0,"max_bytes":33554432},"records":[]}"#;
    fs::write(&path, fixture).expect("invalid v1 traffic history");

    assert!(matches!(
        TrafficStore::open(path.clone()),
        Err(TrafficError::Rotation(_))
    ));
    assert_eq!(fs::read(path).expect("unchanged traffic history"), fixture);
}

#[test]
fn history_is_bucketed_persisted_and_rotated_by_age() {
    let root = tempfile::tempdir().expect("temporary directory");
    let path = root.path().join("traffic.json");
    let store = TrafficStore::open(path.clone()).expect("store");
    store
        .record(
            3_540_000,
            vec![TrafficDelta {
                dimension: TrafficDimension::Host,
                label: "old.example".into(),
                download: 10,
                upload: 2,
            }],
        )
        .expect("old record");
    store
        .record(
            7_200_000,
            vec![TrafficDelta {
                dimension: TrafficDimension::Host,
                label: "new.example".into(),
                download: 20,
                upload: 4,
            }],
        )
        .expect("new record");
    store
        .update_settings(
            TrafficSettings {
                retention_hours: Some(1),
                reset_day: None,
                retention_months: Some(12),
                max_bytes: Some(MIN_MAX_BYTES),
            },
            7_200_000,
        )
        .expect("settings");
    let reopened = TrafficStore::open(path).expect("reopened store");
    let history = reopened
        .history(0, TrafficDimension::Host, 7_200_000)
        .expect("history");
    assert_eq!(history.totals.len(), 1);
    assert_eq!(history.totals[0].label, "new.example");
}

#[test]
fn maximum_size_rotation_drops_oldest_buckets_first() {
    let root = tempfile::tempdir().expect("temporary directory");
    let store = TrafficStore::open(root.path().join("traffic.json")).expect("store");
    for minute in 0..20 {
        store
            .record(
                minute * BUCKET_MILLIS,
                vec![TrafficDelta {
                    dimension: TrafficDimension::Host,
                    label: format!("{minute}-{}", "x".repeat(70_000)),
                    download: 1,
                    upload: 1,
                }],
            )
            .expect("record");
    }
    store
        .update_settings(
            TrafficSettings {
                retention_hours: Some(MAX_RETENTION_HOURS),
                reset_day: None,
                retention_months: Some(12),
                max_bytes: Some(MIN_MAX_BYTES),
            },
            20 * BUCKET_MILLIS,
        )
        .expect("settings");
    let history = store
        .history(0, TrafficDimension::Host, 20 * BUCKET_MILLIS)
        .expect("history");
    assert!(u64::try_from(history.storage_bytes).expect("storage size") <= MIN_MAX_BYTES);
    assert!(history.totals.len() < 20);
    assert!(
        history
            .totals
            .iter()
            .all(|item| !item.label.starts_with("0-"))
    );
}
