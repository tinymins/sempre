use sempre_state::{JsonMigration, MigrationError};
use serde_json::{Map, Value};

pub(super) const MIGRATION: JsonMigration = JsonMigration::new(
    2,
    "v0002_monthly_retention",
    include_str!("v0002_monthly_retention.rs"),
    apply,
);

fn apply(document: &mut Map<String, Value>) -> Result<(), MigrationError> {
    let settings = document
        .get_mut("settings")
        .and_then(Value::as_object_mut)
        .ok_or(MigrationError::InvalidField {
            id: MIGRATION.id(),
            field: "settings",
        })?;
    settings.entry("reset_day").or_insert(Value::Null);
    settings
        .entry("retention_months")
        .or_insert(Value::from(12));
    document.insert("schema".into(), Value::from(2));
    Ok(())
}
