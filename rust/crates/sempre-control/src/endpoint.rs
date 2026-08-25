use std::{fs, path::Path};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use rand::random;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use url::Url;

use crate::{ControlError, validate_listen};

const SCHEMA: u32 = 1;
pub const API_MAJOR: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DaemonEndpoint {
    pub schema: u32,
    pub api_major: u32,
    pub base_url: String,
    pub token: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicEndpoint {
    pub schema: u32,
    pub api_major: u32,
    pub version: String,
    pub bind: String,
    pub local_url: String,
    pub updated_at: DateTime<Utc>,
}

impl DaemonEndpoint {
    pub fn new(base_url: &str) -> Result<Self, ControlError> {
        validate_base_url(base_url)?;
        let token: [u8; 32] = random();
        Ok(Self {
            schema: SCHEMA,
            api_major: API_MAJOR,
            base_url: base_url.trim_end_matches('/').into(),
            token: URL_SAFE_NO_PAD.encode(token),
            updated_at: Utc::now(),
        })
    }

    pub fn write(&self, path: &Path) -> Result<(), ControlError> {
        self.validate()?;
        write_json(path, self, 0o600)
    }

    pub fn read(path: &Path) -> Result<Self, ControlError> {
        let data = fs::read(path).map_err(|error| ControlError::io("read", path, error))?;
        let endpoint: Self = serde_json::from_slice(&data).map_err(ControlError::Decode)?;
        endpoint.validate()?;
        Ok(endpoint)
    }

    fn validate(&self) -> Result<(), ControlError> {
        if self.schema != SCHEMA || self.api_major != API_MAJOR {
            return Err(ControlError::invalid("daemon endpoint is incompatible"));
        }
        validate_base_url(&self.base_url)?;
        let token = URL_SAFE_NO_PAD
            .decode(&self.token)
            .map_err(|_| ControlError::invalid("daemon endpoint token is invalid"))?;
        if token.len() != 32 {
            return Err(ControlError::invalid("daemon endpoint token is invalid"));
        }
        Ok(())
    }
}

impl PublicEndpoint {
    pub fn new(version: &str, bind: &str, local_url: &str) -> Result<Self, ControlError> {
        validate_listen(bind)?;
        validate_base_url(local_url)?;
        Ok(Self {
            schema: SCHEMA,
            api_major: API_MAJOR,
            version: version.into(),
            bind: bind.into(),
            local_url: local_url.trim_end_matches('/').into(),
            updated_at: Utc::now(),
        })
    }

    pub fn write(&self, path: &Path) -> Result<(), ControlError> {
        self.validate()?;
        write_json(path, self, 0o644)
    }

    pub fn read(path: &Path) -> Result<Self, ControlError> {
        let data = fs::read(path).map_err(|error| ControlError::io("read", path, error))?;
        let endpoint: Self = serde_json::from_slice(&data).map_err(ControlError::Decode)?;
        endpoint.validate()?;
        Ok(endpoint)
    }

    fn validate(&self) -> Result<(), ControlError> {
        if self.schema != SCHEMA || self.api_major != API_MAJOR || self.version.is_empty() {
            return Err(ControlError::invalid("public endpoint is incompatible"));
        }
        validate_listen(&self.bind)?;
        validate_base_url(&self.local_url)
    }
}

pub fn token_matches(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && !left.is_empty()
        && bool::from(left.as_bytes().ct_eq(right.as_bytes()))
}

fn validate_base_url(value: &str) -> Result<(), ControlError> {
    let url = Url::parse(value).map_err(|_| ControlError::invalid("invalid control URL"))?;
    if url.scheme() != "http"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ControlError::invalid("invalid control URL"));
    }
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize, mode: u32) -> Result<(), ControlError> {
    let mut data = serde_json::to_vec_pretty(value).map_err(ControlError::Encode)?;
    data.push(b'\n');
    sempre_state::write_atomic(path, &data, mode)
        .map_err(|error| ControlError::io("write", path, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_endpoint_round_trips_a_private_random_token() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("daemon.json");
        let endpoint = DaemonEndpoint::new("http://127.0.0.1:33211").expect("endpoint");
        endpoint.write(&path).expect("write");
        assert_eq!(DaemonEndpoint::read(&path).expect("read"), endpoint);
        assert!(token_matches(&endpoint.token, &endpoint.token));
        assert!(!token_matches(&endpoint.token, "wrong"));

        let public_path = root.path().join("endpoint.json");
        let public = PublicEndpoint::new("0.1.0", "0.0.0.0:33211", "http://127.0.0.1:33211")
            .expect("public endpoint");
        public.write(&public_path).expect("write public endpoint");
        assert_eq!(
            PublicEndpoint::read(&public_path).expect("read public"),
            public
        );
    }
}
