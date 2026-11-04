use serde_json::{Map, Value};

use crate::{JsonMigration, MigrationError};

pub(super) const MIGRATION: JsonMigration = JsonMigration::new(
    4,
    "v0004_last_good_deployment",
    include_str!("v0004_last_good_deployment.rs"),
    apply,
);

#[allow(clippy::unnecessary_wraps)]
fn apply(document: &mut Map<String, Value>) -> Result<(), MigrationError> {
    document.insert("last_good".into(), Value::Null);
    document.insert("rejected_deployment".into(), Value::Null);
    document.insert("schema".into(), Value::from(4));
    Ok(())
}
