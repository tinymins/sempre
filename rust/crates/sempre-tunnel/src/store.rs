use std::{fs, path::PathBuf};

use sempre_state::write_atomic;

use crate::{Config, TunnelError};

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
        let mut config: Config = serde_json::from_slice(&data).map_err(|error| {
            TunnelError::invalid(format!("decode tunnel configuration: {error}"))
        })?;
        config.normalize();
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
