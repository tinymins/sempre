mod v0002_monthly_retention;
mod v0003_rolling_window;

use sempre_state::{
    AppliedMigration, JsonMigration, JsonMigrationOutcome, MigrationError,
    current_migration_ledger, migrate_json, validate_migration_ledger,
};

pub(crate) const CURRENT_SCHEMA: u32 = 3;
const BASELINE_SCHEMA: u32 = 1;
const REGISTRY: &[JsonMigration] = &[
    v0002_monthly_retention::MIGRATION,
    v0003_rolling_window::MIGRATION,
];

pub(crate) fn current_ledger() -> Vec<AppliedMigration> {
    current_migration_ledger(REGISTRY)
}

pub(crate) fn validate_ledger(ledger: &[AppliedMigration]) -> Result<(), MigrationError> {
    validate_migration_ledger(CURRENT_SCHEMA, ledger, REGISTRY)
}

pub(crate) fn run(data: &[u8]) -> Result<JsonMigrationOutcome, MigrationError> {
    migrate_json(data, BASELINE_SCHEMA, CURRENT_SCHEMA, REGISTRY)
}
