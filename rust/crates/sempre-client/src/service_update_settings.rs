use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServiceUpdateSettings {
    pub(crate) schema: u32,
    #[serde(default)]
    pub(crate) allow_prerelease: bool,
}

impl Default for ServiceUpdateSettings {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            allow_prerelease: false,
        }
    }
}

pub(crate) fn read(path: &Path) -> Result<ServiceUpdateSettings, String> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ServiceUpdateSettings::default());
        }
        Err(error) => return Err(format!("read service update settings: {error}")),
    };
    let settings: ServiceUpdateSettings = serde_json::from_slice(&data)
        .map_err(|error| format!("decode service update settings: {error}"))?;
    if settings.schema != SCHEMA_VERSION {
        return Err(format!(
            "service update settings schema must be {SCHEMA_VERSION}"
        ));
    }
    Ok(settings)
}

pub(crate) fn write(path: &Path, allow_prerelease: bool) -> Result<ServiceUpdateSettings, String> {
    let settings = ServiceUpdateSettings {
        schema: SCHEMA_VERSION,
        allow_prerelease,
    };
    let mut data = serde_json::to_vec_pretty(&settings)
        .map_err(|error| format!("encode service update settings: {error}"))?;
    data.push(b'\n');
    sempre_state::write_atomic(path, &data, 0o600)
        .map_err(|error| format!("write service update settings: {error}"))?;
    Ok(settings)
}
