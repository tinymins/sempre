use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppliedMigration {
    pub version: u32,
    pub id: String,
    pub checksum: String,
}

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("decode document for migration: {0}")]
    Decode(#[source] serde_json::Error),
    #[error("migration document must be a JSON object")]
    InvalidDocument,
    #[error("migration document has no valid schema version")]
    InvalidSchema,
    #[error("unsupported document schema {0}")]
    UnsupportedSchema(u32),
    #[error("document schema {0} has no registered migration")]
    MissingMigration(u32),
    #[error("document schema {schema} has an invalid applied migration ledger")]
    InvalidLedger { schema: u32 },
    #[error("migration {id:?} checksum differs from the registered migration")]
    ChecksumDrift { id: String },
    #[error("migration {id:?} cannot migrate invalid field {field:?}")]
    InvalidField {
        id: &'static str,
        field: &'static str,
    },
    #[error("migrated document is invalid: {0}")]
    Validation(String),
}

pub struct JsonMigration {
    version: u32,
    id: &'static str,
    source: &'static str,
    apply: fn(&mut Map<String, Value>) -> Result<(), MigrationError>,
}

pub struct MigrationOutcome {
    pub value: Value,
    pub changed: bool,
}

impl JsonMigration {
    pub const fn new(
        version: u32,
        id: &'static str,
        source: &'static str,
        apply: fn(&mut Map<String, Value>) -> Result<(), MigrationError>,
    ) -> Self {
        Self {
            version,
            id,
            source,
            apply,
        }
    }

    pub const fn id(&self) -> &'static str {
        self.id
    }
}

pub fn current_ledger(registry: &[JsonMigration]) -> Vec<AppliedMigration> {
    registry.iter().map(applied_migration).collect()
}

pub fn validate_ledger(
    schema: u32,
    ledger: &[AppliedMigration],
    registry: &[JsonMigration],
) -> Result<(), MigrationError> {
    let expected: Vec<_> = registry
        .iter()
        .filter(|migration| migration.version <= schema)
        .map(applied_migration)
        .collect();
    if ledger.len() != expected.len() {
        return Err(MigrationError::InvalidLedger { schema });
    }
    for (actual, expected) in ledger.iter().zip(expected) {
        if actual.version != expected.version || actual.id != expected.id {
            return Err(MigrationError::InvalidLedger { schema });
        }
        if actual.checksum != expected.checksum {
            return Err(MigrationError::ChecksumDrift {
                id: actual.id.clone(),
            });
        }
    }
    Ok(())
}

pub fn migrate_json(
    data: &[u8],
    baseline_schema: u32,
    current_schema: u32,
    registry: &[JsonMigration],
) -> Result<MigrationOutcome, MigrationError> {
    let mut value: Value = serde_json::from_slice(data).map_err(MigrationError::Decode)?;
    let object = value
        .as_object_mut()
        .ok_or(MigrationError::InvalidDocument)?;
    let mut schema = object
        .get("schema")
        .and_then(Value::as_u64)
        .and_then(|schema| u32::try_from(schema).ok())
        .ok_or(MigrationError::InvalidSchema)?;
    if schema > current_schema || schema < baseline_schema {
        return Err(MigrationError::UnsupportedSchema(schema));
    }
    let mut ledger = match object.get("applied_migrations") {
        Some(value) => serde_json::from_value(value.clone()).map_err(MigrationError::Decode)?,
        None if schema == baseline_schema => Vec::new(),
        None => return Err(MigrationError::InvalidLedger { schema }),
    };
    validate_ledger(schema, &ledger, registry)?;

    let mut changed = false;
    while schema < current_schema {
        let target = schema + 1;
        let migration = registry
            .iter()
            .find(|migration| migration.version == target)
            .ok_or(MigrationError::MissingMigration(schema))?;
        (migration.apply)(object)?;
        ledger.push(applied_migration(migration));
        object.insert(
            "applied_migrations".into(),
            serde_json::to_value(&ledger).map_err(MigrationError::Decode)?,
        );
        schema = target;
        changed = true;
    }
    Ok(MigrationOutcome { value, changed })
}

fn applied_migration(migration: &JsonMigration) -> AppliedMigration {
    AppliedMigration {
        version: migration.version,
        id: migration.id.into(),
        checksum: format!("{:x}", Sha256::digest(migration.source.as_bytes())),
    }
}
