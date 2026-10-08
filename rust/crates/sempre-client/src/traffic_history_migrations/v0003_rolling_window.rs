use sempre_state::{JsonMigration, MigrationError};
use serde_json::{Map, Value};

pub(super) const MIGRATION: JsonMigration = JsonMigration::new(
    3,
    "v0003_rolling_window",
    include_str!("v0003_rolling_window.rs"),
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
    let previous_retention = settings.get("retention_hours").cloned();
    let window_hours = previous_retention
        .as_ref()
        .filter(|value| value.is_u64())
        .cloned()
        .unwrap_or_else(|| Value::from(24));
    settings.entry("window_hours").or_insert(window_hours);
    if let Some(hours) = previous_retention.as_ref().and_then(Value::as_u64) {
        let retention_hours = if (1..=24 * 30).contains(&hours) {
            hours.max(24 * 30)
        } else {
            0
        };
        settings.insert("retention_hours".into(), Value::from(retention_hours));
    }
    document.insert("schema".into(), Value::from(3));
    Ok(())
}
