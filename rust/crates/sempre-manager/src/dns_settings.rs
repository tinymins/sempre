use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::Write as _,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use sempre_converter::Profile;
use sempre_dns::{DnsQueryEvent, DnsRewrite, DnsRuntimePolicy};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{DnsRoutingRuleSet, ManagerError};

const SCHEMA_VERSION: u32 = 3;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DnsSettings {
    pub schema: u32,
    pub revision: u64,
    pub enabled: bool,
    #[serde(default)]
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
    fn from_profile(profile: &Profile) -> Self {
        let dns = effective_dns(profile);
        let shared = dns.get("shared").unwrap_or(&dns);
        Self {
            schema: SCHEMA_VERSION,
            revision: 1,
            enabled: boolean(shared, "systemDnsTakeoverEnabled", true),
            direct_upstreams: Vec::new(),
            rule_sets: Vec::new(),
            reject_https: boolean(shared, "rejectHttps", true),
            rewrites: Vec::new(),
            query_log_enabled: true,
            query_log_max_entries: default_max_entries(),
        }
    }

    pub(crate) fn requires_core_rebuild(&self, candidate: &Self) -> bool {
        self.enabled != candidate.enabled
            || self.direct_upstreams != candidate.direct_upstreams
            || self.rule_sets != candidate.rule_sets
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
    query_path: PathBuf,
    queries: Mutex<VecDeque<DnsQueryEvent>>,
    query_appends: AtomicUsize,
}

impl DnsSettingsStore {
    pub(crate) fn open(
        path: PathBuf,
        query_path: PathBuf,
        initial_profile: &Profile,
    ) -> Result<Self, ManagerError> {
        let settings = match fs::read(&path) {
            Ok(data) => decode_settings(&data)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let settings = DnsSettings::from_profile(initial_profile);
                write(&path, &settings)?;
                settings
            }
            Err(error) => return Err(ManagerError::io("read DNS settings", error)),
        };
        validate(&settings)?;
        write(&path, &settings)?;
        let queries = load_queries(&query_path, settings.query_log_max_entries)?;
        Ok(Self {
            path,
            settings: Mutex::new(settings),
            query_path,
            queries: Mutex::new(queries),
            query_appends: AtomicUsize::new(0),
        })
    }

    pub(crate) fn read(&self) -> DnsSettings {
        self.settings.lock().expect("DNS settings lock").clone()
    }

    pub(crate) fn replace(&self, mut candidate: DnsSettings) -> Result<DnsSettings, ManagerError> {
        if candidate.schema != SCHEMA_VERSION {
            return Err(ManagerError::InvalidOperation(format!(
                "DNS settings schema must be {SCHEMA_VERSION}"
            )));
        }
        crate::dns_routing::normalize(&mut candidate);
        validate(&candidate)?;
        let mut current = self.settings.lock().expect("DNS settings lock");
        candidate.revision = current.revision.saturating_add(1);
        write(&self.path, &candidate)?;
        *current = candidate.clone();
        let mut queries = self.queries.lock().expect("DNS query log lock");
        while queries.len() > candidate.query_log_max_entries {
            queries.pop_front();
        }
        write_queries(&self.query_path, &queries)?;
        Ok(candidate)
    }

    pub(crate) fn restore(&self, settings: DnsSettings) -> Result<(), ManagerError> {
        write(&self.path, &settings)?;
        *self.settings.lock().expect("DNS settings lock") = settings;
        Ok(())
    }

    pub(crate) fn queries(&self) -> Vec<DnsQueryEvent> {
        self.queries
            .lock()
            .expect("DNS query log lock")
            .iter()
            .rev()
            .cloned()
            .collect()
    }

    pub(crate) fn clear_queries(&self) -> Result<(), ManagerError> {
        self.queries.lock().expect("DNS query log lock").clear();
        self.query_appends.store(0, Ordering::Relaxed);
        sempre_state::write_atomic(&self.query_path, b"", 0o600)
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
        let settings = self.read();
        if !settings.query_log_enabled {
            return;
        }
        let line = serde_json::to_vec(&event).ok();
        let mut queries = self.queries.lock().expect("DNS query log lock");
        queries.push_back(event);
        while queries.len() > settings.query_log_max_entries {
            queries.pop_front();
        }
        if let Some(mut line) = line {
            line.push(b'\n');
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.query_path)
            {
                let _ = file.write_all(&line);
            }
        }
        if self.query_appends.fetch_add(1, Ordering::Relaxed) % 100 == 99 {
            let _ = write_queries(&self.query_path, &queries);
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
        direct_upstreams: Vec::new(),
        rule_sets: Vec::new(),
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
        direct_upstreams: Vec::new(),
        rule_sets: Vec::new(),
        reject_https: boolean(shared, "rejectHttps", true),
        rewrites: legacy.rewrites,
        query_log_enabled: legacy.query_log_enabled,
        query_log_max_entries: legacy.query_log_max_entries,
    })
}

fn effective_dns(profile: &Profile) -> Value {
    serde_json::from_str(&profile.editor.dns_config).unwrap_or_else(|_| profile.dns.clone())
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

fn load_queries(
    path: &std::path::Path,
    limit: usize,
) -> Result<VecDeque<DnsQueryEvent>, ManagerError> {
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(VecDeque::new()),
        Err(error) => return Err(ManagerError::io("read DNS query history", error)),
    };
    let mut queries = data
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect::<VecDeque<_>>();
    while queries.len() > limit {
        queries.pop_front();
    }
    Ok(queries)
}

fn write_queries(
    path: &std::path::Path,
    queries: &VecDeque<DnsQueryEvent>,
) -> Result<(), ManagerError> {
    let mut data = Vec::new();
    for query in queries {
        serde_json::to_writer(&mut data, query).map_err(|error| {
            ManagerError::InvalidOperation(format!("encode DNS query history: {error}"))
        })?;
        data.push(b'\n');
    }
    sempre_state::write_atomic(path, &data, 0o600)
        .map_err(|error| ManagerError::io("write DNS query history", error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_settings_enable_frontend_and_preserve_a_saved_opt_out() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("dns.json");
        let queries = temp.path().join("queries.ndjson");
        let profile = Profile::default();
        let store = DnsSettingsStore::open(path.clone(), queries.clone(), &profile).expect("store");
        assert!(
            store.read().enabled,
            "new installations must enable the DNS frontend"
        );
        let mut disabled = store.read();
        disabled.enabled = false;
        let saved = store.replace(disabled).expect("explicit opt out");
        let reopened = DnsSettingsStore::open(path, queries, &profile).expect("reopen");
        assert_eq!(reopened.read(), saved);
    }

    #[test]
    fn initial_profile_keeps_an_explicit_legacy_opt_out() {
        let mut profile = Profile::default();
        profile.editor.dns_config = r#"{"shared":{"systemDnsTakeoverEnabled":false}}"#.into();
        assert!(!DnsSettings::from_profile(&profile).enabled);
    }

    #[test]
    fn migrates_only_frontend_fields_from_legacy_device_dns() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("dns.json");
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "schema": 1,
                "revision": 7,
                "use_system_dns": false,
                "config": "{\"shared\":{\"systemDnsTakeoverEnabled\":true,\"rejectHttps\":false,\"remoteDns\":\"1.1.1.1\"}}",
                "dns": {},
                "rewrites": [],
                "query_log_enabled": true,
                "query_log_max_entries": 500
            }))
            .expect("legacy settings"),
        )
        .expect("write legacy settings");
        let mut profile = Profile::default();
        profile.editor.dns_config = "{\"shared\":{\"fakeipEnabled\":false}}".into();
        profile
            .extra
            .insert("use_system_dns".into(), Value::Bool(false));
        let store =
            DnsSettingsStore::open(path.clone(), temp.path().join("queries.ndjson"), &profile)
                .expect("store");
        let migrated = store.read();
        assert_eq!(migrated.schema, 3);
        assert!(migrated.enabled);
        assert!(!migrated.reject_https);
        assert_eq!(migrated.query_log_max_entries, 500);
        assert!(
            !fs::read_to_string(&path)
                .expect("migrated file")
                .contains("remoteDns")
        );

        profile.editor.dns_config = "changed by profile".into();
        let reopened = DnsSettingsStore::open(path, temp.path().join("queries.ndjson"), &profile)
            .expect("reopened store");
        assert_eq!(reopened.read(), migrated);
    }

    #[test]
    fn migrates_schema_two_without_importing_profile_dns() {
        let settings = decode_settings(
            &serde_json::to_vec(&serde_json::json!({
                "schema": 2,
                "revision": 9,
                "enabled": true,
                "reject_https": false,
                "rewrites": [],
                "query_log_enabled": true,
                "query_log_max_entries": 800
            }))
            .expect("schema 2 settings"),
        )
        .expect("migration");
        assert_eq!(settings.schema, 3);
        assert_eq!(settings.revision, 10);
        assert!(settings.direct_upstreams.is_empty());
        assert!(settings.rule_sets.is_empty());
        assert!(!settings.reject_https);
    }
}
