use std::{env, net::SocketAddr};

use thiserror::Error;
use url::Url;

#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub database_url: String,
    pub bind_address: SocketAddr,
    pub public_url: Url,
    pub allow_registration: bool,
    pub session_days: i64,
}

#[derive(Debug, Error)]
pub(crate) enum ConfigError {
    #[error("{0} is required")]
    Missing(&'static str),
    #[error("invalid {name}: {detail}")]
    Invalid { name: &'static str, detail: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = required("DATABASE_URL")?;
        if !database_url.starts_with("postgres://") && !database_url.starts_with("postgresql://") {
            return Err(invalid("DATABASE_URL", &"must use PostgreSQL"));
        }
        let bind_address = env::var("SEMPRE_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8787".into())
            .parse()
            .map_err(|error| invalid("SEMPRE_BIND", &error))?;
        let public_url: Url = required("SEMPRE_PUBLIC_URL")?
            .parse()
            .map_err(|error| invalid("SEMPRE_PUBLIC_URL", &error))?;
        if !matches!(public_url.scheme(), "http" | "https") || public_url.cannot_be_a_base() {
            return Err(invalid("SEMPRE_PUBLIC_URL", &"must be an HTTP(S) base URL"));
        }
        let allow_registration = parse_bool("SEMPRE_ALLOW_REGISTRATION", true)?;
        let session_days = env::var("SEMPRE_SESSION_DAYS")
            .unwrap_or_else(|_| "30".into())
            .parse::<i64>()
            .map_err(|error| invalid("SEMPRE_SESSION_DAYS", &error))?;
        if !(1..=365).contains(&session_days) {
            return Err(invalid("SEMPRE_SESSION_DAYS", &"must be between 1 and 365"));
        }
        Ok(Self {
            database_url,
            bind_address,
            public_url,
            allow_registration,
            session_days,
        })
    }
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ConfigError::Missing(name))
}

fn parse_bool(name: &'static str, default: bool) -> Result<bool, ConfigError> {
    match env::var(name).ok().as_deref() {
        None => Ok(default),
        Some("1" | "true" | "yes" | "on") => Ok(true),
        Some("0" | "false" | "no" | "off") => Ok(false),
        Some(_) => Err(invalid(name, &"must be true or false")),
    }
}

fn invalid(name: &'static str, detail: &impl ToString) -> ConfigError {
    ConfigError::Invalid {
        name,
        detail: detail.to_string(),
    }
}
