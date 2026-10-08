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
