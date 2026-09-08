use sempre_state::{
    AppliedMigration, JsonMigration, MigrationError, current_migration_ledger, migrate_json,
    validate_migration_ledger,
};
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};

const LF: &str = "migration source\nwith a second line\n";
const CRLF: &str = "migration source\r\nwith a second line\r\n";

fn must_not_run(_: &mut Map<String, Value>) -> Result<(), MigrationError> {
    panic!("an applied migration must not run again")
}

fn registry(source: &'static str) -> [JsonMigration; 1] {
    [JsonMigration::new(2, "migration", source, must_not_run)]
}

fn legacy(source: &str) -> Vec<AppliedMigration> {
    vec![AppliedMigration {
        version: 2,
        id: "migration".into(),
        checksum: format!("{:x}", Sha256::digest(source.as_bytes())),
    }]
}

#[test]
fn new_migration_checksums_are_independent_of_checkout_line_endings() {
    assert_eq!(
        current_migration_ledger(&registry(LF)),
        current_migration_ledger(&registry(CRLF))
    );
}

#[test]
fn either_legacy_checkout_can_be_read_without_reapplying_or_rewriting_data() {
    for source in [LF, CRLF] {
        for recorded in [LF, CRLF] {
            let value = json!({
                "schema": 2,
                "applied_migrations": legacy(recorded),
                "preserved_data": {"enabled": true}
            });
            let result = migrate_json(
                &serde_json::to_vec(&value).unwrap(),
                1,
                2,
                &registry(source),
            )
            .unwrap();
            assert!(!result.changed);
            assert_eq!(result.value, value);
        }
    }
}

#[test]
fn content_change_is_still_rejected() {
    assert!(matches!(
        validate_migration_ledger(2, &legacy("different migration\n"), &registry(LF)),
        Err(MigrationError::ChecksumDrift { .. })
    ));
}
