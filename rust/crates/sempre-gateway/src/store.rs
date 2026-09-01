use std::{fs, path::PathBuf, sync::OnceLock};

use sempre_state::{Layout, validate_json_shape, write_atomic};

use crate::{Config, GatewayError, model::DhcpReservation};

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(layout: &Layout) -> Self {
        Self {
            path: layout.gateway.join("config.json"),
        }
    }

    pub fn initialize(&self) -> Result<Config, GatewayError> {
        if self.path.exists() {
            let data = fs::read(&self.path)
                .map_err(|error| GatewayError::io("read gateway configuration", error))?;
            let (config, migrated) = decode(&data)?;
            if migrated {
                self.write(&config)
            } else {
                Ok(config)
            }
        } else {
            let config = Config::default();
            self.write(&config)?;
            Ok(config)
        }
    }

    pub fn read(&self) -> Result<Config, GatewayError> {
        let data = fs::read(&self.path)
            .map_err(|error| GatewayError::io("read gateway configuration", error))?;
        decode(&data).map(|(config, _)| config)
    }

    pub fn write(&self, config: &Config) -> Result<Config, GatewayError> {
        let mut config = config.clone();
        config.normalize();
        config.validate()?;
        let mut data = serde_json::to_vec_pretty(&config).map_err(|error| {
            GatewayError::invalid(format!("encode gateway configuration: {error}"))
        })?;
        data.push(b'\n');
        write_atomic(&self.path, &data, 0o600)
            .map_err(|error| GatewayError::io("write gateway configuration", error))?;
        Ok(config)
    }
}

fn config_shape() -> &'static serde_json::Value {
    static SHAPE: OnceLock<serde_json::Value> = OnceLock::new();
    SHAPE.get_or_init(|| {
        let mut config = Config::default();
        config.dhcp.reservations.push(DhcpReservation::default());
        serde_json::to_value(config).expect("gateway configuration shape")
    })
}

fn decode(data: &[u8]) -> Result<(Config, bool), GatewayError> {
    let mut value = serde_json::from_slice::<serde_json::Value>(data)
        .map_err(|error| GatewayError::invalid(format!("decode gateway configuration: {error}")))?;
    let migrated = value.get("schema").and_then(serde_json::Value::as_u64) == Some(1);
    if migrated {
        let object = value.as_object_mut().ok_or_else(|| {
            GatewayError::invalid("decode gateway configuration: expected an object")
        })?;
        object.remove("dns");
        object.insert(
            "schema".into(),
            u64::from(crate::model::SCHEMA_VERSION).into(),
        );
    }
    validate_json_shape(&value, config_shape())
        .map_err(|error| GatewayError::invalid(format!("decode gateway configuration: {error}")))?;
    let config: Config = serde_json::from_value(value)
        .map_err(|error| GatewayError::invalid(format!("decode gateway configuration: {error}")))?;
    config.validate()?;
    Ok((config, migrated))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use sempre_state::Layout;

    use super::Store;

    #[test]
    fn initialize_migrates_schema_one_and_removes_dns() {
        let root = tempfile::tempdir().expect("temporary directory");
        let layout = Layout::at(root.path());
        let store = Store::new(&layout);
        let mut document = serde_json::to_value(crate::Config::default()).expect("config value");
        document["schema"] = 1.into();
        document["dns"] = serde_json::json!({ "enabled": true });
        let data = serde_json::to_vec_pretty(&document).expect("config data");
        fs::create_dir_all(&layout.gateway).expect("gateway directory");
        fs::write(layout.gateway.join("config.json"), &data).expect("gateway config");

        let migrated = store.initialize().expect("migrate config");
        assert_eq!(migrated.schema, 2);
        let stored: serde_json::Value = serde_json::from_slice(
            &fs::read(layout.gateway.join("config.json")).expect("stored config"),
        )
        .expect("stored JSON");
        assert!(stored.get("dns").is_none());
    }

    #[test]
    fn read_requires_every_current_field_and_preserves_valid_bytes() {
        let root = tempfile::tempdir().expect("temporary directory");
        let layout = Layout::at(root.path());
        let store = Store::new(&layout);
        let config = crate::Config::default();
        let mut incomplete = serde_json::to_value(&config).expect("config value");
        incomplete["dhcp"]
            .as_object_mut()
            .expect("DHCP object")
            .remove("lease_time");
        fs::create_dir_all(&layout.gateway).expect("gateway directory");
        fs::write(
            layout.gateway.join("config.json"),
            serde_json::to_vec_pretty(&incomplete).expect("incomplete config"),
        )
        .expect("gateway config");
        assert!(store.read().is_err());

        let data = serde_json::to_vec_pretty(&config).expect("config data");
        fs::write(layout.gateway.join("config.json"), &data).expect("gateway config");
        assert_eq!(store.read().expect("strict current config"), config);
        assert_eq!(
            fs::read(layout.gateway.join("config.json")).expect("stored config"),
            data
        );
    }
}
