use serde_json::{Value, json};

use super::*;

const BETA2_CHECKSUMS: [&str; 2] = [
    "08dc71a5c51739768636910627db9229078a5cb173bdbfdad027dbd0539a4ef3",
    "bf0bc14805ceecd7f878e8cc0b6f963f196ee365bae0d51bea2a360abfc0df4b",
];

pub(crate) fn beta2_document(checksum: &str) -> Value {
    let mut ledger = current_ledger();
    ledger[1].checksum = checksum.into();
    json!({
        "schema": 3,
        "applied_migrations": ledger,
        "settings": {
            "window_hours": 24, "retention_hours": 720,
            "reset_day": null, "retention_months": 12, "max_bytes": null
        },
        "records": [{
            "time": 1_791_417_600_000_i64, "dimension": "host", "label": "example.com",
            "download": 1234, "upload": 567
        }]
    })
}

#[test]
fn beta2_history_opens_without_reapplying_migrations_or_rewriting_data() {
    for checksum in BETA2_CHECKSUMS {
        let document = beta2_document(checksum);
        let bytes = serde_json::to_vec_pretty(&document).unwrap();
        let result = run(&bytes).expect("beta.2 migration remains valid");
        assert!(!result.changed);
        assert_eq!(result.value, document);

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("traffic-history.json");
        std::fs::write(&path, &bytes).unwrap();
        let _store = crate::traffic_history::TrafficStore::open(path.clone()).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn recording_traffic_preserves_the_applied_migration_ledger() {
    use crate::traffic_history::{TrafficDelta, TrafficDimension, TrafficStore};

    for checksum in BETA2_CHECKSUMS {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("traffic-history.json");
        let document = beta2_document(checksum);
        std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        let store = TrafficStore::open(path.clone()).unwrap();
        store
            .record(
                1_791_417_600_000,
                vec![TrafficDelta {
                    dimension: TrafficDimension::Host,
                    label: "example.com".into(),
                    download: 10,
                    upload: 20,
                }],
            )
            .unwrap();
        store.flush().unwrap();
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["applied_migrations"], document["applied_migrations"]);
        assert_eq!(saved["records"][0]["download"], 1244);
        assert_eq!(saved["records"][0]["upload"], 587);
        TrafficStore::open(path).unwrap();
    }
}

#[test]
fn unknown_checksum_is_still_rejected() {
    let bytes = serde_json::to_vec(&beta2_document("unrecognized-checksum")).unwrap();
    assert!(matches!(
        run(&bytes),
        Err(MigrationError::ChecksumDrift { .. })
    ));
}

#[test]
fn rolling_window_migration_preserves_records_and_promotes_retention() {
    let mut document = beta2_document(BETA2_CHECKSUMS[0]);
    document["schema"] = json!(2);
    document["applied_migrations"].as_array_mut().unwrap().pop();
    document["settings"]
        .as_object_mut()
        .unwrap()
        .remove("window_hours");
    document["settings"]["retention_hours"] = json!(24);
    let result = run(&serde_json::to_vec(&document).unwrap()).unwrap();
    assert!(result.changed);
    assert_eq!(result.value["schema"], 3);
    assert_eq!(result.value["settings"]["window_hours"], 24);
    assert_eq!(result.value["settings"]["retention_hours"], 720);
    assert_eq!(result.value["records"], document["records"]);
}
