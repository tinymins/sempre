use std::{fs, path::PathBuf, sync::OnceLock};

use sempre_state::{Layout, validate_json_shape, write_atomic};

use crate::{
    Config, GatewayError,
    model::{DhcpReservation, DnsRuleSet},
};

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
            self.read()
        } else {
            let config = Config::default();
            self.write(&config)?;
            Ok(config)
        }
    }

    pub fn read(&self) -> Result<Config, GatewayError> {
        let data = fs::read(&self.path)
            .map_err(|error| GatewayError::io("read gateway configuration", error))?;
        let value = serde_json::from_slice(&data).map_err(|error| {
            GatewayError::invalid(format!("decode gateway configuration: {error}"))
        })?;
        validate_json_shape(&value, config_shape()).map_err(|error| {
            GatewayError::invalid(format!("decode gateway configuration: {error}"))
        })?;
        let config: Config = serde_json::from_value(value).map_err(|error| {
            GatewayError::invalid(format!("decode gateway configuration: {error}"))
        })?;
        config.validate()?;
        Ok(config)
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
        config.dns.rule_sets.push(DnsRuleSet::default());
        serde_json::to_value(config).expect("gateway configuration shape")
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use sempre_state::Layout;

    use super::Store;

    #[test]
    fn read_rejects_old_schema_without_rewriting_the_file() {
        let root = tempfile::tempdir().expect("temporary directory");
        let layout = Layout::at(root.path());
        let store = Store::new(&layout);
        let mut document = serde_json::to_value(crate::Config::default()).expect("config value");
        document["schema"] = 0.into();
        let data = serde_json::to_vec_pretty(&document).expect("config data");
        fs::create_dir_all(&layout.gateway).expect("gateway directory");
        fs::write(layout.gateway.join("config.json"), &data).expect("gateway config");

        assert!(store.read().is_err());
        assert_eq!(
            fs::read(layout.gateway.join("config.json")).expect("stored config"),
            data
        );
    }

    #[test]
    fn read_requires_every_current_field_and_preserves_valid_bytes() {
        let root = tempfile::tempdir().expect("temporary directory");
        let layout = Layout::at(root.path());
        let store = Store::new(&layout);
        let mut config = crate::Config::default();
        config.dns.listen_hosts.clear();
        let mut incomplete = serde_json::to_value(&config).expect("config value");
        incomplete["dns"]
            .as_object_mut()
            .expect("DNS object")
            .remove("cache_ttl_seconds");
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
