mod v0002_pending_change_contract;

use crate::{
    AppliedMigration, Document, MigrationError,
    model::SCHEMA_VERSION,
    schema_migration::{JsonMigration, migrate_json},
};

pub(crate) struct MigrationOutcome {
    pub(crate) document: Document,
    pub(crate) changed: bool,
}

const BASELINE_SCHEMA: u32 = 1;
const REGISTRY: &[JsonMigration] = &[v0002_pending_change_contract::MIGRATION];

pub(crate) fn current_ledger() -> Vec<AppliedMigration> {
    crate::schema_migration::current_ledger(REGISTRY)
}

pub(crate) fn validate_ledger(ledger: &[AppliedMigration]) -> Result<(), MigrationError> {
    crate::schema_migration::validate_ledger(SCHEMA_VERSION, ledger, REGISTRY)
}

pub(crate) fn run(data: &[u8]) -> Result<MigrationOutcome, MigrationError> {
    let migration = migrate_json(data, BASELINE_SCHEMA, SCHEMA_VERSION, REGISTRY)?;
    let document: Document =
        serde_json::from_value(migration.value).map_err(MigrationError::Decode)?;
    document
        .validate()
        .map_err(|error| MigrationError::Validation(error.to_string()))?;
    validate_ledger(&document.applied_migrations)?;
    Ok(MigrationOutcome {
        document,
        changed: migration.changed,
    })
}
