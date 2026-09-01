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
use sempre_gateway::{DnsQueryEvent, DnsRewrite, DnsRuntimePolicy};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ManagerError;

const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DnsSettings {
    pub schema: u32,
    pub revision: u64,
    pub use_system_dns: bool,
    pub config: String,
    #[serde(default)]
    pub dns: Value,
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
        Self {
            schema: SCHEMA_VERSION,
            revision: 1,
            use_system_dns: profile
                .extra
                .get("use_system_dns")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            config: profile.editor.dns_config.clone(),
            dns: profile.dns.clone(),
            rewrites: Vec::new(),
            query_log_enabled: true,
            query_log_max_entries: default_max_entries(),
        }
    }

    pub(crate) fn apply(&self, profile: &mut Profile) {
        profile
            .extra
            .insert("use_system_dns".into(), Value::Bool(self.use_system_dns));
        profile.editor.dns_config.clone_from(&self.config);
        profile.dns.clone_from(&self.dns);
    }

    pub(crate) fn matches(&self, profile: &Profile) -> bool {
        self.use_system_dns
            == profile
                .extra
                .get("use_system_dns")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            && self.config == profile.editor.dns_config
            && self.dns == profile.dns
    }
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
            Ok(data) => serde_json::from_slice::<DnsSettings>(&data).map_err(|error| {
                ManagerError::InvalidOperation(format!("decode DNS settings: {error}"))
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let settings = DnsSettings::from_profile(initial_profile);
                write(&path, &settings)?;
                settings
            }
            Err(error) => return Err(ManagerError::io("read DNS settings", error)),
        };
        if settings.schema != SCHEMA_VERSION {
            return Err(ManagerError::InvalidOperation(format!(
                "DNS settings schema {} is not supported",
                settings.schema
            )));
        }
        validate(&settings)?;
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
    fn migrates_profile_dns_once_and_keeps_it_independent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut profile = Profile::default();
        profile.editor.dns_config = "{\"shared\":{\"fakeipEnabled\":false}}".into();
        profile
            .extra
            .insert("use_system_dns".into(), Value::Bool(false));
        let store = DnsSettingsStore::open(
            temp.path().join("dns.json"),
            temp.path().join("queries.ndjson"),
            &profile,
        )
        .expect("store");
        let migrated = store.read();
        assert!(!migrated.use_system_dns);
        assert_eq!(migrated.config, profile.editor.dns_config);

        profile.editor.dns_config = "changed by profile".into();
        let reopened = DnsSettingsStore::open(
            temp.path().join("dns.json"),
            temp.path().join("queries.ndjson"),
            &profile,
        )
        .expect("reopened store");
        assert_eq!(reopened.read().config, migrated.config);
    }
}
