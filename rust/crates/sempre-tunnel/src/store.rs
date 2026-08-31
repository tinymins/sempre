use std::{fs, path::PathBuf, sync::OnceLock};

use sempre_state::{validate_json_shape, write_atomic};

use crate::{Config, Forward, Instance, TunnelError};

pub(crate) struct Store {
    path: PathBuf,
}

impl Store {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn initialize(&self) -> Result<Config, TunnelError> {
        if self.path.exists() {
            self.read()
        } else {
            let config = Config::default();
            self.write(&config)?;
            Ok(config)
        }
    }

    pub(crate) fn read(&self) -> Result<Config, TunnelError> {
        let data = fs::read(&self.path)
            .map_err(|error| TunnelError::io("read tunnel configuration", error))?;
        let value = serde_json::from_slice(&data).map_err(|error| {
            TunnelError::invalid(format!("decode tunnel configuration: {error}"))
        })?;
        validate_json_shape(&value, config_shape()).map_err(|error| {
            TunnelError::invalid(format!("decode tunnel configuration: {error}"))
        })?;
        let config: Config = serde_json::from_value(value).map_err(|error| {
            TunnelError::invalid(format!("decode tunnel configuration: {error}"))
        })?;
        config.validate()?;
        Ok(config)
    }

    pub(crate) fn write(&self, config: &Config) -> Result<(), TunnelError> {
        let mut config = config.clone();
        config.normalize();
        config.validate()?;
        let mut data = serde_json::to_vec_pretty(&config).map_err(|error| {
            TunnelError::invalid(format!("encode tunnel configuration: {error}"))
        })?;
        data.push(b'\n');
        write_atomic(&self.path, &data, 0o600)
            .map_err(|error| TunnelError::io("write tunnel configuration", error))
    }
}

fn config_shape() -> &'static serde_json::Value {
    static SHAPE: OnceLock<serde_json::Value> = OnceLock::new();
    SHAPE.get_or_init(|| {
        serde_json::to_value(Config {
            schema: crate::model::SCHEMA_VERSION,
            instances: vec![Instance {
                id: String::new(),
                name: String::new(),
                desired_state: String::new(),
                server_url: String::new(),
                dns_resolvers: Vec::new(),
                prefer_ipv4: false,
                websocket_ping: String::new(),
                connection_retry_max_backoff: String::new(),
                upgrade_path_prefix: String::new(),
                forwards: vec![Forward {
                    id: String::new(),
                    name: String::new(),
                    listen_port: 0,
                    remote_host: String::new(),
                    remote_port: 0,
                    timeout_seconds: 0,
                }],
            }],
        })
        .expect("tunnel configuration shape")
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::Store;

    fn current_config() -> crate::Config {
        crate::Config {
            schema: crate::model::SCHEMA_VERSION,
            instances: vec![crate::Instance {
                id: "hz".into(),
                name: "Hangzhou".into(),
                desired_state: "stopped".into(),
                server_url: "wss://hz.example.com".into(),
                dns_resolvers: Vec::new(),
                prefer_ipv4: false,
                websocket_ping: "15s".into(),
                connection_retry_max_backoff: "30s".into(),
                upgrade_path_prefix: String::new(),
                forwards: vec![crate::Forward {
                    id: "hz-wg".into(),
                    name: "WG".into(),
                    listen_port: 52_001,
                    remote_host: "127.0.0.1".into(),
                    remote_port: 31_088,
                    timeout_seconds: 0,
                }],
            }],
        }
    }

    #[test]
    fn read_rejects_old_schema_without_rewriting_the_file() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("tunnels.json");
        let store = Store::new(path.clone());
        let mut document = serde_json::to_value(current_config()).expect("config value");
        document["schema"] = 0.into();
        let data = serde_json::to_vec_pretty(&document).expect("config data");
        fs::write(&path, &data).expect("tunnel config");

        assert!(store.read().is_err());
        assert_eq!(fs::read(path).expect("stored config"), data);
    }

    #[test]
    fn read_requires_every_current_field_and_preserves_valid_bytes() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("tunnels.json");
        let store = Store::new(path.clone());
        let mut config = current_config();
        config.instances[0].name = " Hangzhou ".into();
        let mut incomplete = serde_json::to_value(&config).expect("config value");
        incomplete["instances"][0]
            .as_object_mut()
            .expect("instance object")
            .remove("upgrade_path_prefix");
        fs::write(
            &path,
            serde_json::to_vec_pretty(&incomplete).expect("incomplete config"),
        )
        .expect("tunnel config");
        assert!(store.read().is_err());

        let data = serde_json::to_vec_pretty(&config).expect("config data");
        fs::write(&path, &data).expect("tunnel config");
        assert_eq!(store.read().expect("strict current config"), config);
        assert_eq!(fs::read(path).expect("stored config"), data);
    }
}
