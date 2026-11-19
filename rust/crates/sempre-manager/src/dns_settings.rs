use std::{fs, io, path::PathBuf, sync::Mutex};

use sempre_converter::Profile;
use sempre_dns::{DnsQueryEvent, DnsRewrite, DnsRuntimePolicy};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{DnsRoutingRuleSet, ManagerError, dns_query_history::QueryHistory};

const SCHEMA_VERSION: u32 = 3;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DnsSettings {
    pub schema: u32,
    pub revision: u64,
    pub enabled: bool,
    #[serde(default)]
    pub domestic_domains: sempre_dns::DomesticDomainPolicy,
    #[serde(default = "sempre_dns::default_upstreams")]
    pub direct_upstreams: Vec<String>,
    #[serde(default)]
    pub rule_sets: Vec<DnsRoutingRuleSet>,
    #[serde(default = "default_true")]
    pub reject_https: bool,
    #[serde(default)]
    pub rewrites: Vec<DnsRewrite>,
    #[serde(default = "default_true")]
    pub query_log_enabled: bool,
    #[serde(default = "default_max_entries")]
    pub query_log_max_entries: usize,
}

const fn default_true() -> bool {
    true
}

const fn default_max_entries() -> usize {
    2_000
}

impl DnsSettings {
    pub(crate) fn normalize_and_validate(&mut self) -> Result<(), ManagerError> {
        if self.schema != SCHEMA_VERSION {
            return Err(ManagerError::InvalidOperation(format!(
                "DNS settings schema must be {SCHEMA_VERSION}"
            )));
        }
        crate::dns_routing::normalize(self);
        validate(self)
    }

    fn from_profile(profile: &Profile) -> Self {
        let dns = effective_dns(profile);
        let shared = dns.get("shared").unwrap_or(&dns);
        Self {
            schema: SCHEMA_VERSION,
            revision: 1,
            enabled: boolean(shared, "systemDnsTakeoverEnabled", true),
            direct_upstreams: sempre_dns::default_upstreams(),
            rule_sets: Vec::new(),
            domestic_domains: sempre_dns::DomesticDomainPolicy::default(),
            reject_https: boolean(shared, "rejectHttps", true),
            rewrites: Vec::new(),
            query_log_enabled: true,
            query_log_max_entries: default_max_entries(),
        }
    }

    pub(crate) fn requires_core_rebuild(&self, candidate: &Self) -> bool {
        self.enabled != candidate.enabled
            || self.rule_sets != candidate.rule_sets
            || self.domestic_domains != candidate.domestic_domains
    }
}

#[derive(Deserialize)]
struct LegacyDnsSettings {
    revision: u64,
    #[serde(default)]
    config: String,
    #[serde(default)]
    dns: Value,
    #[serde(default)]
    rewrites: Vec<DnsRewrite>,
    #[serde(default = "default_true")]
    query_log_enabled: bool,
    #[serde(default = "default_max_entries")]
    query_log_max_entries: usize,
}

#[derive(Deserialize)]
struct V2DnsSettings {
    revision: u64,
    enabled: bool,
    #[serde(default = "default_true")]
    reject_https: bool,
    #[serde(default)]
    rewrites: Vec<DnsRewrite>,
    #[serde(default = "default_true")]
    query_log_enabled: bool,
    #[serde(default = "default_max_entries")]
    query_log_max_entries: usize,
}

pub(crate) struct DnsSettingsStore {
    path: PathBuf,
    settings: Mutex<DnsSettings>,
    administration: Mutex<()>,
    history: QueryHistory,
}

impl DnsSettingsStore {
    pub(crate) fn open(
        path: PathBuf,
        query_path: PathBuf,
        log_path: PathBuf,
        initial_profile: &Profile,
    ) -> Result<Self, ManagerError> {
        let mut settings = match fs::read(&path) {
            Ok(data) => decode_settings(&data)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let settings = DnsSettings::from_profile(initial_profile);
                write(&path, &settings)?;
                settings
            }
            Err(error) => return Err(ManagerError::io("read DNS settings", error)),
        };
        crate::dns_routing::normalize(&mut settings);
        validate(&settings)?;
        write(&path, &settings)?;
        let history = QueryHistory::open(query_path, settings.query_log_max_entries, log_path)
            .map_err(|error| ManagerError::io("open DNS query history", error))?;
        Ok(Self {
            path,
            settings: Mutex::new(settings),
            administration: Mutex::new(()),
            history,
        })
    }

    pub(crate) fn read(&self) -> DnsSettings {
        self.settings.lock().expect("DNS settings lock").clone()
    }

    pub(crate) fn replace(&self, mut candidate: DnsSettings) -> Result<DnsSettings, ManagerError> {
        candidate.normalize_and_validate()?;
        let _admin = self.administration.lock().expect("DNS administration lock");
        let previous = self.read();
        candidate.revision = previous.revision.saturating_add(1);
        let limit = candidate.query_log_max_entries;
        let changed_limit = previous.query_log_max_entries != limit;
        let path = self.path.clone();
        let next = candidate.clone();
        let save = move || {
            if let Err(error) = write(&path, &next) {
                return match write(&path, &previous) {
                    Ok(()) => Err(io::Error::other(error)),
                    Err(rollback) => Err(io::Error::other(format!(
                        "DNS settings persistence failed: {error}; restoring saved settings failed: {rollback}"
                    ))),
                };
            }
            Ok(())
        };
        if changed_limit {
            self.history.resize_and_save(limit, save)
        } else {
            save()
        }
        .map_err(|error| ManagerError::io("update DNS settings", error))?;
        *self.settings.lock().expect("DNS settings lock") = candidate.clone();
        Ok(candidate)
    }

    pub(crate) fn queries(&self) -> Vec<DnsQueryEvent> {
        self.history.recent()
    }

    pub(crate) fn clear_queries(&self) -> Result<(), ManagerError> {
        let _admin = self.administration.lock().expect("DNS administration lock");
        self.history
            .clear()
            .map_err(|error| ManagerError::io("clear DNS query history", error))
    }
}

impl DnsRuntimePolicy for DnsSettingsStore {
    fn rewrite(&self, name: &str, record_type: &str) -> Option<DnsRewrite> {
        let settings = self.read();
        let name = name.trim_end_matches('.').to_ascii_lowercase();
        settings.rewrites.into_iter().find(|rule| {
            if !rule.enabled || !rule.record_type.eq_ignore_ascii_case(record_type) {
                return false;
            }
            let domain = rule.domain.trim_end_matches('.').to_ascii_lowercase();
            domain.strip_prefix("*.").map_or(name == domain, |suffix| {
                name.len() > suffix.len() && name.ends_with(&format!(".{suffix}"))
            })
        })
    }

    fn reject_https(&self) -> bool {
        self.read().reject_https
    }

    fn record(&self, event: DnsQueryEvent) {
        if self
            .settings
            .lock()
            .expect("DNS settings lock")
            .query_log_enabled
        {
            self.history.record(event);
        }
    }
}

fn decode_settings(data: &[u8]) -> Result<DnsSettings, ManagerError> {
    let value = serde_json::from_slice::<Value>(data)
        .map_err(|error| ManagerError::InvalidOperation(format!("decode DNS settings: {error}")))?;
    match value.get("schema").and_then(Value::as_u64) {
        Some(version) if version == u64::from(SCHEMA_VERSION) => serde_json::from_value(value)
            .map_err(|error| {
                ManagerError::InvalidOperation(format!("decode DNS settings: {error}"))
            }),
        Some(2) => migrate_v2(value),
        Some(1) => migrate_legacy(value),
        version => Err(ManagerError::InvalidOperation(format!(
            "DNS settings schema {} is not supported",
            version.map_or_else(|| "missing".into(), |value| value.to_string())
        ))),
    }
}

fn migrate_v2(value: Value) -> Result<DnsSettings, ManagerError> {
    let previous = serde_json::from_value::<V2DnsSettings>(value).map_err(|error| {
        ManagerError::InvalidOperation(format!("decode DNS settings schema 2: {error}"))
    })?;
    Ok(DnsSettings {
        schema: SCHEMA_VERSION,
        revision: previous.revision.saturating_add(1),
        enabled: previous.enabled,
        direct_upstreams: sempre_dns::default_upstreams(),
        rule_sets: Vec::new(),
        domestic_domains: sempre_dns::DomesticDomainPolicy::default(),
        reject_https: previous.reject_https,
        rewrites: previous.rewrites,
        query_log_enabled: previous.query_log_enabled,
        query_log_max_entries: previous.query_log_max_entries,
    })
}

fn migrate_legacy(value: Value) -> Result<DnsSettings, ManagerError> {
    let legacy = serde_json::from_value::<LegacyDnsSettings>(value).map_err(|error| {
        ManagerError::InvalidOperation(format!("decode legacy DNS settings: {error}"))
    })?;
    let dns = serde_json::from_str::<Value>(&legacy.config).unwrap_or(legacy.dns);
    let shared = dns.get("shared").unwrap_or(&dns);
    Ok(DnsSettings {
        schema: SCHEMA_VERSION,
        revision: legacy.revision.saturating_add(1),
        enabled: boolean(shared, "systemDnsTakeoverEnabled", false),
        direct_upstreams: sempre_dns::default_upstreams(),
        rule_sets: Vec::new(),
        domestic_domains: sempre_dns::DomesticDomainPolicy::default(),
        reject_https: boolean(shared, "rejectHttps", true),
        rewrites: legacy.rewrites,
        query_log_enabled: legacy.query_log_enabled,
        query_log_max_entries: legacy.query_log_max_entries,
    })
}

fn effective_dns(profile: &Profile) -> Value {
    sempre_converter::profile_from_editor(profile).map_or(Value::Null, |profile| profile.dns)
}

fn boolean(value: &Value, key: &str, fallback: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

fn write(path: &std::path::Path, settings: &DnsSettings) -> Result<(), ManagerError> {
    let data = serde_json::to_vec_pretty(settings)
        .map_err(|error| ManagerError::InvalidOperation(format!("encode DNS settings: {error}")))?;
    sempre_state::write_atomic(path, &data, 0o600)
        .map_err(|error| ManagerError::io("write DNS settings", error))
}

fn validate(settings: &DnsSettings) -> Result<(), ManagerError> {
    if !(100..=20_000).contains(&settings.query_log_max_entries) {
        return Err(ManagerError::InvalidOperation(
            "DNS query log limit must be between 100 and 20000".into(),
        ));
    }
    crate::dns_routing::validate(settings)?;
    for rule in &settings.rewrites {
        if rule.id.trim().is_empty()
            || rule.domain.trim().is_empty()
            || rule.answer.trim().is_empty()
        {
            return Err(ManagerError::InvalidOperation(
                "DNS rewrite id, domain, and answer are required".into(),
            ));
        }
        match rule.record_type.to_ascii_uppercase().as_str() {
            "A" if rule.answer.parse::<std::net::Ipv4Addr>().is_ok() => {}
            "AAAA" if rule.answer.parse::<std::net::Ipv6Addr>().is_ok() => {}
            "CNAME" if !rule.answer.trim().is_empty() => {}
            _ => {
                return Err(ManagerError::InvalidOperation(format!(
                    "DNS rewrite {} has an invalid type or answer",
                    rule.id
                )));
            }
        }
    }
    Ok(())
}
