use serde_json::{Map, Value};

use crate::{JsonMigration, MigrationError};

pub(super) const MIGRATION: JsonMigration = JsonMigration::new(
    3,
    "v0003_remove_runtime_rollback",
    include_str!("v0003_remove_runtime_rollback.rs"),
    apply,
);

fn apply(document: &mut Map<String, Value>) -> Result<(), MigrationError> {
    document.remove("previous");
    document.remove("previous_config_build");
    document.remove("previous_profile_id");
    let runtime = document
        .get_mut("runtime")
        .and_then(Value::as_object_mut)
        .ok_or(MigrationError::InvalidField {
            id: MIGRATION.id(),
            field: "runtime",
        })?;
    if let Some(failure) = runtime.get_mut("last_failure")
        && !failure.is_null()
    {
        failure
            .as_object_mut()
            .ok_or(MigrationError::InvalidField {
                id: MIGRATION.id(),
                field: "runtime.last_failure",
            })?
            .remove("rolled_back_to");
    }
    document.insert("schema".into(), Value::from(3));
    Ok(())
}
