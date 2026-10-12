use std::{
    env,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

use thiserror::Error;
use url::Url;

#[derive(Clone)]
pub(crate) struct Config {
    pub database_url: String,
    pub bind_address: SocketAddr,
    pub public_url: Url,
    pub access_log_retention_days: i64,
    pub web_root: PathBuf,
    pub proxy: Option<crate::proxy_runtime::ProxyConfig>,
    pub trusted_proxy_ips: Vec<IpAddr>,
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
        let access_log_retention_days = env::var("SEMPRE_ACCESS_LOG_RETENTION_DAYS")
            .unwrap_or_else(|_| "90".into())
            .parse::<i64>()
            .map_err(|error| invalid("SEMPRE_ACCESS_LOG_RETENTION_DAYS", &error))?;
        if !(1..=3650).contains(&access_log_retention_days) {
            return Err(invalid(
                "SEMPRE_ACCESS_LOG_RETENTION_DAYS",
                &"must be between 1 and 3650",
            ));
        }
        let proxy = match env::var("SEMPRE_PROXY_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
        {
            Some(value) => {
                let mut url: Url = value
                    .parse()
                    .map_err(|error| invalid("SEMPRE_PROXY_URL", &error))?;
                if !matches!(url.scheme(), "http" | "https")
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                {
                    return Err(invalid(
                        "SEMPRE_PROXY_URL",
                        &"must be an HTTP(S) runtime URL without credentials",
                    ));
                }
                if !url.path().ends_with('/') {
                    url.set_path(&format!("{}/", url.path()));
                }
                let token = required("SEMPRE_PROXY_TOKEN")?;
                if token.len() < 32 {
                    return Err(invalid(
                        "SEMPRE_PROXY_TOKEN",
                        &"must contain at least 32 characters",
                    ));
                }
                let key = crate::proxy_crypto::key(&required("SEMPRE_PROXY_KEY")?)
                    .map_err(|error| invalid("SEMPRE_PROXY_KEY", &error))?;
                Some(crate::proxy_runtime::ProxyConfig { url, token, key })
            }
            None => None,
        };
        let trusted_proxy_ips = env::var("SEMPRE_TRUSTED_PROXY_IPS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                value
                    .parse()
                    .map_err(|error| invalid("SEMPRE_TRUSTED_PROXY_IPS", &error))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            database_url,
            bind_address,
            public_url,
            access_log_retention_days,
            web_root: env::var("SEMPRE_WEB_ROOT")
                .map_or_else(|_| PathBuf::from("server-ui/dist"), PathBuf::from),
            proxy,
            trusted_proxy_ips,
        })
    }
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ConfigError::Missing(name))
}

fn invalid(name: &'static str, detail: &impl ToString) -> ConfigError {
    ConfigError::Invalid {
        name,
        detail: detail.to_string(),
    }
}
