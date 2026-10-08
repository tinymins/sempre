use sempre_state::{AppliedMigration, JsonMigration, MigrationError, validate_migration_ledger};
use serde_json::{Map, Value};

const CURRENT: &str = "migration source\n";
const CURRENT_HASH: &str = "a9cf93e01d224dee37a3099fc9897136c3a6f7394a5e6d783429daf84bc353ef";
const LEGACY: &str = "legacy-checksum";

fn must_not_run(_: &mut Map<String, Value>) -> Result<(), MigrationError> {
    panic!("already applied migration")
}

fn migration(source: &'static str) -> JsonMigration {
    JsonMigration::new(2, "migration", source, must_not_run)
        .with_legacy_checksums(&[(CURRENT_HASH, LEGACY)])
}

#[test]
fn legacy_checksum_is_scoped_to_the_pinned_source_and_migration() {
    let mut ledger = vec![AppliedMigration {
        version: 2,
        id: "migration".into(),
        checksum: LEGACY.into(),
    }];
    validate_migration_ledger(2, &ledger, &[migration(CURRENT)]).unwrap();
    assert!(matches!(
        validate_migration_ledger(2, &ledger, &[migration("changed migration source\n")]),
        Err(MigrationError::ChecksumDrift { .. })
    ));
    assert!(matches!(
        validate_migration_ledger(
            2,
            &ledger,
            &[JsonMigration::new(2, "migration", CURRENT, must_not_run)]
        ),
        Err(MigrationError::ChecksumDrift { .. })
    ));
    ledger[0].id = "another-migration".into();
    assert!(matches!(
        validate_migration_ledger(2, &ledger, &[migration(CURRENT)]),
        Err(MigrationError::InvalidLedger { .. })
    ));
}
