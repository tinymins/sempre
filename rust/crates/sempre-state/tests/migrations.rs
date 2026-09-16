use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
};

use sempre_state::{Document, Layout, MigrationError, PendingConfigField, StateError, Store};
use serde_json::Value;

fn store() -> (tempfile::TempDir, Store) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let store = Store::new(Layout::at(temporary.path()));
    store.layout().ensure().expect("state layout");
    (temporary, store)
}

fn v1_fixture(pending_config_fields: Option<Vec<PendingConfigField>>) -> Vec<u8> {
    let mut value = serde_json::to_value(Document::default()).expect("serialize fixture");
    let object = value.as_object_mut().expect("state object");
    object.insert("schema".into(), Value::from(1));
    object.remove("applied_migrations");
    object.remove("previous_config_build");
    object.remove("previous_profile_id");
    match pending_config_fields {
        Some(fields) => {
            object.insert(
                "pending_config_fields".into(),
                serde_json::to_value(fields).expect("serialize pending fields"),
            );
        }
        None => {
            object.remove("pending_config_fields");
        }
    }
    serde_json::to_vec_pretty(&value).expect("encode fixture")
}

#[test]
fn initialize_migrates_v1_once_and_preserves_existing_pending_fields() {
    for expected in [Vec::new(), vec![PendingConfigField::Dns]] {
        let (_temporary, store) = store();
        let fixture = if expected.is_empty() {
            v1_fixture(None)
        } else {
            v1_fixture(Some(expected.clone()))
        };
        fs::write(&store.layout().state, fixture).expect("write v1 state");

        let migrated = store.initialize().expect("migrate state");
        assert_eq!(migrated.schema, 3);
        assert_eq!(migrated.pending_config_fields, expected);
        assert_eq!(migrated.applied_migrations.len(), 2);
        assert_eq!(
            migrated.applied_migrations[0].id,
            "v0002_pending_change_contract"
        );
        assert_eq!(
            migrated.applied_migrations[1].id,
            "v0003_remove_runtime_rollback"
        );
    }
}

#[test]
fn second_initialize_leaves_migrated_state_byte_identical() {
    let (_temporary, store) = store();
    fs::write(&store.layout().state, v1_fixture(None)).expect("write v1 state");
    store.initialize().expect("first initialize");
    let first = fs::read(&store.layout().state).expect("first state bytes");

    store.initialize().expect("second initialize");
    let second = fs::read(&store.layout().state).expect("second state bytes");
    assert_eq!(second, first);
}

#[test]
fn v3_migration_removes_every_runtime_rollback_target() {
    let (_temporary, store) = store();
    let mut value = serde_json::to_value(Document::default()).expect("serialize state");
    value["schema"] = Value::from(2);
    value["applied_migrations"]
        .as_array_mut()
        .expect("migration ledger")
        .truncate(1);
    let object = value.as_object_mut().expect("state object");
    object.insert(
        "previous".into(),
        serde_json::json!({
            "core": "sing-box",
            "repository": null,
            "reference": "stable",
            "version": "1.13.2",
            "config_hash": "a".repeat(64),
        }),
    );
    object.insert("previous_config_build".into(), Value::Null);
    object.insert(
        "previous_profile_id".into(),
        Value::String("old-profile".into()),
    );
    object
        .get_mut("runtime")
        .and_then(Value::as_object_mut)
        .expect("runtime")
        .insert(
            "last_failure".into(),
            serde_json::json!({
                "stage": "startup failed",
                "error": "exit status 1",
                "occurred_at": "2026-09-16T00:00:00Z",
                "failed": null,
                "rolled_back_to": null,
            }),
        );
    fs::write(
        &store.layout().state,
        serde_json::to_vec_pretty(&value).expect("encode v2 state"),
    )
    .expect("write v2 state");

    let migrated = store.initialize().expect("migrate v2 state");
    assert_eq!(migrated.schema, 3);
    let encoded = serde_json::to_value(migrated).expect("serialize migrated state");
    assert!(encoded.get("previous").is_none());
    assert!(encoded.get("previous_config_build").is_none());
    assert!(encoded.get("previous_profile_id").is_none());
    assert!(
        encoded["runtime"]["last_failure"]
            .get("rolled_back_to")
            .is_none()
    );
}

#[test]
fn future_schema_is_rejected_without_modifying_state() {
    let (_temporary, store) = store();
    let mut value = serde_json::to_value(Document::default()).expect("serialize state");
    value["schema"] = Value::from(99);
    let fixture = serde_json::to_vec_pretty(&value).expect("encode fixture");
    fs::write(&store.layout().state, &fixture).expect("write future state");

    assert!(matches!(
        store.initialize(),
        Err(StateError::Migration(MigrationError::UnsupportedSchema(99)))
    ));
    assert_eq!(
        fs::read(&store.layout().state).expect("unchanged state"),
        fixture
    );
}

#[test]
fn checksum_drift_is_rejected_without_modifying_state() {
    let (_temporary, store) = store();
    let mut value = serde_json::to_value(Document::default()).expect("serialize state");
    value["applied_migrations"][0]["checksum"] = Value::String("0".repeat(64));
    let fixture = serde_json::to_vec_pretty(&value).expect("encode fixture");
    fs::write(&store.layout().state, &fixture).expect("write drifted state");

    assert!(matches!(
        store.initialize(),
        Err(StateError::Migration(MigrationError::ChecksumDrift { .. }))
    ));
    assert_eq!(
        fs::read(&store.layout().state).expect("unchanged state"),
        fixture
    );
}

#[test]
fn failed_migration_does_not_persist_schema_or_ledger() {
    let (_temporary, store) = store();
    let mut value: Value = serde_json::from_slice(&v1_fixture(None)).expect("decode v1 fixture");
    value["pending_config_fields"] = Value::String("not-a-list".into());
    let fixture = serde_json::to_vec_pretty(&value).expect("encode malformed fixture");
    fs::write(&store.layout().state, &fixture).expect("write malformed state");

    assert!(matches!(
        store.initialize(),
        Err(StateError::Migration(MigrationError::InvalidField {
            field: "pending_config_fields",
            ..
        }))
    ));
    assert_eq!(
        fs::read(&store.layout().state).expect("unchanged state"),
        fixture
    );
}

#[test]
fn read_rejects_v1_without_running_migrations() {
    let (_temporary, store) = store();
    fs::write(
        &store.layout().state,
        v1_fixture(Some(vec![PendingConfigField::Dns])),
    )
    .expect("write v1 state");
    assert!(store.read().is_err());
    let value: Value =
        serde_json::from_slice(&fs::read(&store.layout().state).expect("read unchanged v1 state"))
            .expect("decode unchanged v1 state");
    assert_eq!(value["schema"], 1);
}

#[test]
fn concurrent_initialize_applies_the_migration_once() {
    let (_temporary, store) = store();
    fs::write(&store.layout().state, v1_fixture(None)).expect("write v1 state");
    let store = Arc::new(store);
    let barrier = Arc::new(Barrier::new(3));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                store.initialize().expect("concurrent initialize")
            })
        })
        .collect();
    barrier.wait();

    for handle in handles {
        let document = handle.join().expect("initialize thread");
        assert_eq!(document.applied_migrations.len(), 2);
    }
    let persisted = store.read().expect("read migrated state");
    assert_eq!(persisted.applied_migrations.len(), 2);
}
