mod v0002_monthly_retention;
mod v0003_rolling_window;

#[cfg(test)]
pub(crate) mod tests;

use sempre_state::{
    AppliedMigration, JsonMigration, JsonMigrationOutcome, MigrationError,
    current_migration_ledger, migrate_json, validate_migration_ledger,
};

pub(crate) const CURRENT_SCHEMA: u32 = 3;
const BASELINE_SCHEMA: u32 = 1;
const REGISTRY: &[JsonMigration] = &[
    v0002_monthly_retention::MIGRATION,
    // beta.2 included inline tests in this source. Their removal changed the hash
    // without changing apply(); retain only the verified LF/CRLF historical hashes.
    v0003_rolling_window::MIGRATION.with_legacy_checksums(&[
        (
            "41ac28fc6cff9d9d294da8192fbae8375d9630815ac4e3e1ea7846fd498d37c4",
            "08dc71a5c51739768636910627db9229078a5cb173bdbfdad027dbd0539a4ef3",
        ),
        (
            "41ac28fc6cff9d9d294da8192fbae8375d9630815ac4e3e1ea7846fd498d37c4",
            "bf0bc14805ceecd7f878e8cc0b6f963f196ee365bae0d51bea2a360abfc0df4b",
        ),
    ]),
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
