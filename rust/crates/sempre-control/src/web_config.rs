use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use rand::random;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use url::{Host, Url};

use crate::ControlError;

const SCHEMA: u32 = 1;
const MEMORY_KIB: u32 = 64 * 1024;
const ITERATIONS: u32 = 3;
const PARALLELISM: u32 = 2;
const KEY_LENGTH: usize = 32;
pub const DEFAULT_LISTEN: &str = "127.0.0.1:33211";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WebConfig {
    pub schema: u32,
    pub listen: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<PasswordRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PasswordRecord {
    algorithm: String,
    version: u32,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: String,
    key: String,
}

#[derive(Clone)]
pub struct WebConfigStore {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            listen: DEFAULT_LISTEN.into(),
            password: None,
        }
    }
}

impl WebConfig {
    pub fn password_protected(&self) -> bool {
        self.password.is_some()
    }

    pub fn verify_password(&self, password: &str) -> bool {
        self.password
            .as_ref()
            .is_none_or(|record| record.verify(password))
    }

    fn validate(&self) -> Result<(), ControlError> {
        if self.schema != SCHEMA {
            return Err(ControlError::invalid(format!(
                "unsupported web configuration schema {}",
                self.schema
            )));
        }
        validate_listen(&self.listen)?;
        if let Some(record) = &self.password {
            record.decode()?;
        }
        Ok(())
    }
}

impl WebConfigStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn initialize(&self) -> Result<WebConfig, ControlError> {
        let _guard = self.lock.lock().expect("web config lock");
        match self.read_unlocked() {
            Ok(config) => Ok(config),
            Err(ControlError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                let config = WebConfig::default();
                self.write_unlocked(&config)?;
                Ok(config)
            }
            Err(error) => Err(error),
        }
    }

    pub fn read(&self) -> Result<WebConfig, ControlError> {
        let _guard = self.lock.lock().expect("web config lock");
        self.read_unlocked()
    }

    pub fn set_listen(&self, listen: &str) -> Result<WebConfig, ControlError> {
        validate_listen(listen)?;
        self.update(|config| config.listen = listen.into())
    }

    pub fn set_password(&self, password: &str) -> Result<WebConfig, ControlError> {
        let password = (!password.is_empty())
            .then(|| PasswordRecord::new(password))
            .transpose()?;
        self.update(|config| config.password = password)
    }

    fn update(&self, change: impl FnOnce(&mut WebConfig)) -> Result<WebConfig, ControlError> {
        let _guard = self.lock.lock().expect("web config lock");
        let mut config = self.read_unlocked()?;
        change(&mut config);
        config.schema = SCHEMA;
        config.validate()?;
        self.write_unlocked(&config)?;
        Ok(config)
    }

    fn read_unlocked(&self) -> Result<WebConfig, ControlError> {
        let data =
            fs::read(&self.path).map_err(|error| ControlError::io("read", &self.path, error))?;
        let config: WebConfig = serde_json::from_slice(&data).map_err(ControlError::Decode)?;
        config.validate()?;
        Ok(config)
    }

    fn write_unlocked(&self, config: &WebConfig) -> Result<(), ControlError> {
        let mut data = serde_json::to_vec_pretty(config).map_err(ControlError::Encode)?;
        data.push(b'\n');
        sempre_state::write_atomic(&self.path, &data, 0o600)
            .map_err(|error| ControlError::io("write", &self.path, error))
    }
}

impl PasswordRecord {
    fn new(password: &str) -> Result<Self, ControlError> {
        let salt: [u8; 16] = random();
        let mut key = [0_u8; KEY_LENGTH];
        argon2()
            .and_then(|argon| argon.hash_password_into(password.as_bytes(), &salt, &mut key))
            .map_err(|error| ControlError::Password(error.to_string()))?;
        Ok(Self {
            algorithm: "argon2id".into(),
            version: 19,
            memory_kib: MEMORY_KIB,
            iterations: ITERATIONS,
            parallelism: PARALLELISM,
            salt: STANDARD_NO_PAD.encode(salt),
            key: STANDARD_NO_PAD.encode(key),
        })
    }

    fn verify(&self, password: &str) -> bool {
        let Ok((salt, expected)) = self.decode() else {
            return false;
        };
        let mut actual = [0_u8; KEY_LENGTH];
        argon2()
            .and_then(|argon| argon.hash_password_into(password.as_bytes(), &salt, &mut actual))
            .is_ok()
            && bool::from(actual.ct_eq(expected.as_slice()))
    }

    fn decode(&self) -> Result<(Vec<u8>, Vec<u8>), ControlError> {
        if self.algorithm != "argon2id"
            || self.version != 19
            || self.memory_kib != MEMORY_KIB
            || self.iterations != ITERATIONS
            || self.parallelism != PARALLELISM
        {
            return Err(ControlError::invalid(
                "unsupported password record parameters",
            ));
        }
        let salt = STANDARD_NO_PAD
            .decode(&self.salt)
            .map_err(|_| ControlError::invalid("invalid password salt"))?;
        let key = STANDARD_NO_PAD
            .decode(&self.key)
            .map_err(|_| ControlError::invalid("invalid password key"))?;
        if salt.len() != 16 || key.len() != KEY_LENGTH {
            return Err(ControlError::invalid("invalid password record length"));
        }
        Ok((salt, key))
    }
}

fn argon2() -> Result<Argon2<'static>, argon2::Error> {
    let params = Params::new(MEMORY_KIB, ITERATIONS, PARALLELISM, Some(KEY_LENGTH))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

pub fn validate_listen(value: &str) -> Result<(), ControlError> {
    parse_listen(value).map(|_| ())
}

pub fn local_url(value: &str) -> Result<String, ControlError> {
    let mut url = parse_listen(value)?;
    match url.host() {
        Some(Host::Ipv4(address)) if address.is_unspecified() => {
            url.set_host(Some("127.0.0.1"))
                .map_err(|_| ControlError::invalid("invalid IPv4 listen address"))?;
        }
        Some(Host::Ipv6(address)) if address.is_unspecified() => {
            url.set_host(Some("[::1]"))
                .map_err(|_| ControlError::invalid("invalid IPv6 listen address"))?;
        }
        _ => {}
    }
    Ok(url.as_str().trim_end_matches('/').into())
}

fn parse_listen(value: &str) -> Result<Url, ControlError> {
    if value.trim() != value || value.is_empty() {
        return Err(ControlError::invalid(
            "listen address cannot be empty or contain surrounding whitespace",
        ));
    }
    let url = Url::parse(&format!("http://{value}"))
        .map_err(|_| ControlError::invalid("listen address must be host:port"))?;
    if url.host().is_none()
        || url.port().is_none_or(|port| port == 0)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ControlError::invalid("listen address must be host:port"));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_password_without_plaintext_and_verifies_it() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("web.json");
        let store = WebConfigStore::new(&path);
        store.initialize().expect("initialize");
        let config = store.set_password("administrator").expect("password");
        assert!(config.password_protected());
        assert!(config.verify_password("administrator"));
        assert!(!config.verify_password("wrong"));
        assert!(!String::from_utf8_lossy(&fs::read(path).expect("file")).contains("administrator"));
    }

    #[test]
    fn validates_listen_and_maps_wildcards_to_loopback() {
        assert_eq!(
            local_url("0.0.0.0:33211").expect("IPv4"),
            "http://127.0.0.1:33211"
        );
        assert_eq!(local_url("[::]:33211").expect("IPv6"), "http://[::1]:33211");
        assert!(validate_listen("localhost:33211").is_ok());
        for invalid in ["", " 127.0.0.1:1", "127.0.0.1", ":33211", "127.0.0.1:0"] {
            assert!(validate_listen(invalid).is_err(), "{invalid}");
        }
    }
}
