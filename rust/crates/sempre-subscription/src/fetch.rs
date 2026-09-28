use std::{env, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::{Client, Proxy, redirect::Policy};
use sempre_converter::{Source, SourceSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{SubscriptionError, SubscriptionStore};

mod observation;
mod rule_sets;
pub use observation::FetchObservation;
use observation::download_once;
pub use rule_sets::RuleSetSnapshot;

const DEFAULT_USER_AGENT: &str = "clash.meta";
const DEFAULT_FETCH_MODE: &str = "auto";

#[derive(Clone, Debug)]
pub struct FetchResult {
    pub snapshot: SourceSnapshot,
    pub source: Source,
    pub from_cache: bool,
    pub bytes: usize,
}

pub struct FetchInspection {
    pub result: Result<FetchResult, SubscriptionError>,
    pub observation: FetchObservation,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CacheEntry {
    url: String,
    user_agent: String,
    fetch_mode: String,
    snapshot_hash: String,
    fetched_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct Fetcher {
    store: SubscriptionStore,
    standard: Client,
}

impl Fetcher {
    pub fn new(store: SubscriptionStore) -> Result<Self, SubscriptionError> {
        Ok(Self {
            store,
            standard: client(None)?,
        })
    }

    pub async fn load(
        &self,
        source: Source,
        force: bool,
        validate: impl Fn(&str) -> Result<(), SubscriptionError>,
    ) -> Result<FetchResult, SubscriptionError> {
        self.load_inner(source, force, validate, true).await.result
    }

    pub async fn inspect(
        &self,
        source: Source,
        force: bool,
        validate: impl Fn(&str) -> Result<(), SubscriptionError>,
    ) -> Result<FetchResult, SubscriptionError> {
        self.load_inner(source, force, validate, false).await.result
    }

    pub async fn inspect_observed(
        &self,
        source: Source,
        force: bool,
        validate: impl Fn(&str) -> Result<(), SubscriptionError>,
    ) -> FetchInspection {
        self.load_inner(source, force, validate, false).await
    }

    async fn load_inner(
        &self,
        source: Source,
        force: bool,
        validate: impl Fn(&str) -> Result<(), SubscriptionError>,
        persist: bool,
    ) -> FetchInspection {
        let mut observation = FetchObservation::default();
        let result = self
            .load_inner_observed(source, force, validate, persist, &mut observation)
            .await;
        FetchInspection {
            result,
            observation,
        }
    }

    async fn load_inner_observed(
        &self,
        source: Source,
        force: bool,
        validate: impl Fn(&str) -> Result<(), SubscriptionError>,
        persist: bool,
        observation: &mut FetchObservation,
    ) -> Result<FetchResult, SubscriptionError> {
        if source.kind == "raw" {
            return self.raw(source, validate, persist);
        }
        let mut source = source;
        let user_agent = defaulted(&source.user_agent, DEFAULT_USER_AGENT).to_owned();
        let fetch_mode = extra_string(&source, "fetch_mode", DEFAULT_FETCH_MODE).to_owned();
        let key = format!("{}\0{user_agent}\0{fetch_mode}", source.url);
        let cached = self.read_cache(&key).ok();
        let ttl = extra_i64(&source, "cache_ttl_minutes").max(0);
        if !force
            && ttl > 0
            && let Some(entry) = &cached
            && Utc::now() < entry.fetched_at + chrono::Duration::minutes(ttl)
            && let Ok(content) = self.store.read_blob(&entry.snapshot_hash)
            && let Ok(text) = std::str::from_utf8(&content)
            && validate(text).is_ok()
        {
            set_metadata(&mut source, entry, "fresh cache", None);
            return Ok(result(source, text, &entry.snapshot_hash, true));
        }

        let downloaded = self
            .download_observed(&source, &user_agent, &fetch_mode)
            .await;
        let downloaded = match downloaded {
            Ok((content, observed)) => {
                *observation = observed;
                Ok(content)
            }
            Err((error, observed)) => {
                *observation = observed;
                Err(error)
            }
        };
        let validated = downloaded.and_then(|content| {
            let checked = match std::str::from_utf8(&content) {
                Ok(text) => validate(text).map_err(|error| {
                    SubscriptionError::Fetch(format!("downloaded content is unusable: {error}"))
                }),
                Err(_) => Err(SubscriptionError::Fetch("response is not UTF-8".into())),
            };
            if checked.is_err() {
                observation.body_bytes = Some(content.len());
                observation.body_truncated = content.len() > 16_384;
                observation.body_preview = Some(
                    String::from_utf8_lossy(&content[..content.len().min(16_384)]).into_owned(),
                );
            }
            checked?;
            Ok(content)
        });
        match validated {
            Ok(content) => {
                let text = std::str::from_utf8(&content).expect("validated UTF-8 response");
                let hash = if persist {
                    self.store.save_blob(&content)?
                } else {
                    format!("{:x}", Sha256::digest(&content))
                };
                let entry = CacheEntry {
                    url: source.url.clone(),
                    user_agent,
                    fetch_mode,
                    snapshot_hash: hash.clone(),
                    fetched_at: Utc::now(),
                };
                if persist {
                    self.write_cache(&key, &entry)?;
                }
                set_metadata(&mut source, &entry, "downloaded", None);
                Ok(result(source, text, &hash, false))
            }
            Err(error) => {
                if let Some(entry) = cached
                    && let Ok(content) = self.store.read_blob(&entry.snapshot_hash)
                    && let Ok(text) = std::str::from_utf8(&content)
                    && validate(text).is_ok()
                {
                    set_metadata(
                        &mut source,
                        &entry,
                        "last-known-good cache",
                        Some(&error.to_string()),
                    );
                    return Ok(result(source, text, &entry.snapshot_hash, true));
                }
                Err(error)
            }
        }
    }

    pub fn load_cached(
        &self,
        mut source: Source,
        validate: impl Fn(&str) -> Result<(), SubscriptionError>,
    ) -> Result<FetchResult, SubscriptionError> {
        if source.kind == "raw" {
            return self.raw(source, validate, false);
        }
        let hash = extra_string(&source, "snapshot_hash", "").to_owned();
        if hash.is_empty() {
            return Err(SubscriptionError::Fetch(
                "no local subscription snapshot; update the subscription first".into(),
            ));
        }
        let content = self.store.read_blob(&hash)?;
        let text = std::str::from_utf8(&content)
            .map_err(|_| SubscriptionError::Fetch("snapshot is not UTF-8".into()))?;
        validate(text)?;
        let entry = CacheEntry {
            url: source.url.clone(),
            user_agent: source.user_agent.clone(),
            fetch_mode: extra_string(&source, "fetch_mode", DEFAULT_FETCH_MODE).into(),
            snapshot_hash: hash.clone(),
            fetched_at: extra_time(&source, "fetched_at").unwrap_or_else(Utc::now),
        };
        set_metadata(&mut source, &entry, "local snapshot", None);
        Ok(result(source, text, &hash, true))
    }

    fn raw(
        &self,
        mut source: Source,
        validate: impl Fn(&str) -> Result<(), SubscriptionError>,
        persist: bool,
    ) -> Result<FetchResult, SubscriptionError> {
        validate(&source.content)?;
        let hash = if persist {
            self.store.save_blob(source.content.as_bytes())?
        } else {
            format!("{:x}", Sha256::digest(source.content.as_bytes()))
        };
        let entry = CacheEntry {
            url: String::new(),
            user_agent: String::new(),
            fetch_mode: "raw".into(),
            snapshot_hash: hash.clone(),
            fetched_at: Utc::now(),
        };
        set_metadata(&mut source, &entry, "raw content", None);
        Ok(result(source.clone(), &source.content, &hash, false))
    }

    async fn download(
        &self,
        source: &Source,
        user_agent: &str,
        mode: &str,
    ) -> Result<Vec<u8>, SubscriptionError> {
        self.download_observed(source, user_agent, mode)
            .await
            .map(|(content, _)| content)
            .map_err(|(error, _)| error)
    }

    async fn download_observed(
        &self,
        source: &Source,
        user_agent: &str,
        mode: &str,
    ) -> Result<(Vec<u8>, FetchObservation), (SubscriptionError, FetchObservation)> {
        let dynamic;
        let http = if mode == "domestic-direct" {
            dynamic = client(Some(
                domestic_proxy().map_err(|error| (error, FetchObservation::default()))?,
            ))
            .map_err(|error| (error, FetchObservation::default()))?;
            &dynamic
        } else if mode == DEFAULT_FETCH_MODE {
            &self.standard
        } else {
            return Err((
                SubscriptionError::Fetch(format!("unsupported fetch mode {mode:?}")),
                FetchObservation::default(),
            ));
        };
        let mut failures = Vec::new();
        let mut last_observation = FetchObservation::default();
        for attempt in 1..=3 {
            match download_once(http, &source.url, user_agent).await {
                Ok((content, mut observation)) => {
                    observation.attempts = attempt;
                    return Ok((content, observation));
                }
                Err((error, mut observation)) => {
                    observation.attempts = attempt;
                    last_observation = observation;
                    failures.push(format!("attempt {attempt}: {error}"));
                }
            }
        }
        Err((
            SubscriptionError::Fetch(format!(
                "download failed after 3 attempts: {}",
                failures.join("; ")
            )),
            last_observation,
        ))
    }

    fn read_cache(&self, key: &str) -> Result<CacheEntry, SubscriptionError> {
        let data =
            std::fs::read(self.store.cache_path(key)).map_err(SubscriptionError::ReadCache)?;
        serde_json::from_slice(&data).map_err(SubscriptionError::DecodeCache)
    }

    fn write_cache(&self, key: &str, entry: &CacheEntry) -> Result<(), SubscriptionError> {
        let mut data = serde_json::to_vec_pretty(entry).map_err(SubscriptionError::EncodeCache)?;
        data.push(b'\n');
        sempre_state::write_atomic(&self.store.cache_path(key), &data, 0o600)
            .map_err(SubscriptionError::WriteCache)
    }
}

fn client(proxy: Option<Proxy>) -> Result<Client, SubscriptionError> {
    let mut builder = Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(Policy::custom(|attempt| {
            let target = attempt.url();
            if attempt.previous().len() >= 10 {
                return attempt.error("too many redirects");
            }
            if !matches!(target.scheme(), "http" | "https")
                || target.host_str().is_none()
                || !target.username().is_empty()
                || target.password().is_some()
            {
                return attempt.error("refuse invalid redirect target");
            }
            attempt.follow()
        }));
    if let Some(proxy) = proxy {
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|error| SubscriptionError::Fetch(error.to_string()))
}

fn domestic_proxy() -> Result<Proxy, SubscriptionError> {
    let url = env::var("DIRECT_PROXY_URL").map_err(|_| {
        SubscriptionError::Fetch("domestic-direct requires DIRECT_PROXY_URL".into())
    })?;
    let parsed = url::Url::parse(&url)
        .map_err(|_| SubscriptionError::Fetch("DIRECT_PROXY_URL is invalid".into()))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(SubscriptionError::Fetch(
            "DIRECT_PROXY_URL is invalid".into(),
        ));
    }
    let mut proxy =
        Proxy::all(parsed.as_str()).map_err(|error| SubscriptionError::Fetch(error.to_string()))?;
    match (
        env::var("DIRECT_PROXY_USERNAME").ok(),
        env::var("DIRECT_PROXY_PASSWORD").ok(),
    ) {
        (Some(username), Some(password)) if !username.is_empty() && !password.is_empty() => {
            proxy = proxy.basic_auth(&username, &password);
        }
        (None, None) => {}
        _ => {
            return Err(SubscriptionError::Fetch(
                "DIRECT_PROXY_USERNAME and DIRECT_PROXY_PASSWORD must be configured together"
                    .into(),
            ));
        }
    }
    Ok(proxy)
}

fn result(source: Source, content: &str, hash: &str, from_cache: bool) -> FetchResult {
    FetchResult {
        snapshot: SourceSnapshot {
            source_id: source.id.clone(),
            content: content.into(),
            content_hash: hash.into(),
        },
        bytes: content.len(),
        source,
        from_cache,
    }
}

fn set_metadata(source: &mut Source, entry: &CacheEntry, status: &str, error: Option<&str>) {
    source.extra.insert(
        "snapshot_hash".into(),
        Value::String(entry.snapshot_hash.clone()),
    );
    source.extra.insert(
        "fetched_at".into(),
        Value::String(entry.fetched_at.to_rfc3339()),
    );
    source
        .extra
        .insert("last_status".into(), Value::String(status.into()));
    source.extra.insert(
        "last_error".into(),
        Value::String(error.unwrap_or_default().into()),
    );
}

fn defaulted<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

fn extra_string<'a>(source: &'a Source, key: &str, fallback: &'a str) -> &'a str {
    source
        .extra
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(fallback)
}

fn extra_i64(source: &Source, key: &str) -> i64 {
    source.extra.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn extra_time(source: &Source, key: &str) -> Option<DateTime<Utc>> {
    extra_string(source, key, "").parse().ok()
}

#[cfg(test)]
mod tests;
