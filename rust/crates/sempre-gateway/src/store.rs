use std::{fs, path::PathBuf};

use sempre_state::{Layout, write_atomic};

use crate::{Config, GatewayError};

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
        let mut config: Config = serde_json::from_slice(&data).map_err(|error| {
            GatewayError::invalid(format!("decode gateway configuration: {error}"))
        })?;
        config.normalize();
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
