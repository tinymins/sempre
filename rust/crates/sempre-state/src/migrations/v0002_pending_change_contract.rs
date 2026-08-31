use serde_json::{Map, Value};

use crate::{JsonMigration, MigrationError};

pub(super) const MIGRATION: JsonMigration = JsonMigration::new(
    2,
    "v0002_pending_change_contract",
    include_str!("v0002_pending_change_contract.rs"),
    apply,
);

fn apply(document: &mut Map<String, Value>) -> Result<(), MigrationError> {
    if document
        .get("pending_config_fields")
        .is_some_and(|fields| !fields.is_array())
    {
        return Err(MigrationError::InvalidField {
            id: MIGRATION.id(),
            field: "pending_config_fields",
        });
    }
    document
        .entry("pending_config_fields")
        .or_insert_with(|| Value::Array(Vec::new()));
    document
        .entry("previous_config_build")
        .or_insert(Value::Null);
    document.entry("previous_profile_id").or_insert(Value::Null);
    document.insert("schema".into(), Value::from(2));
    Ok(())
}
