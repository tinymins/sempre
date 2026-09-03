use std::{io, net::SocketAddr, path::PathBuf};

use super::{
    CacheEntry, DEFAULT_FETCH_MODE, DEFAULT_USER_AGENT, FetchResult, Fetcher, Proxy, Source,
    SubscriptionError, Utc, client, set_metadata,
};

#[derive(Clone)]
pub struct RuleSetSnapshot {
    pub content: Vec<u8>,
    pub path: PathBuf,
    pub fetched_at: chrono::DateTime<Utc>,
}

impl Fetcher {
    pub fn via_local_http_proxy(
        &self,
        address: SocketAddr,
        username: &str,
        password: &str,
    ) -> Result<Self, SubscriptionError> {
        if !address.ip().is_loopback() {
            return Err(SubscriptionError::Invalid(
                "core proxy must be loopback".into(),
            ));
        }
        let mut proxy = Proxy::all(format!("http://{address}"))
            .map_err(|error| SubscriptionError::Fetch(error.to_string()))?;
        if !username.is_empty() {
            proxy = proxy.basic_auth(username, password);
        }
        Ok(Self {
            store: self.store.clone(),
            standard: client(Some(proxy))?,
        })
    }

    /// Startup uses the last validated snapshot regardless of its refresh TTL.
    pub fn cached_rule_provider(
        &self,
        mut source: Source,
    ) -> Result<FetchResult, SubscriptionError> {
        let key = format!("{}\0{DEFAULT_USER_AGENT}\0{DEFAULT_FETCH_MODE}", source.url);
        let entry = self.read_cache(&key)?;
        set_metadata(&mut source, &entry, "local snapshot", None);
        self.load_cached(source, |content| {
            if sempre_converter::rule_provider_has_rules(content) {
                Ok(())
            } else {
                Err(SubscriptionError::Invalid(
                    "rule snapshot has no usable rules".into(),
                ))
            }
        })
    }

    pub fn cached_rule_set(
        &self,
        url: &str,
        format: &str,
    ) -> Result<Option<RuleSetSnapshot>, SubscriptionError> {
        let entry = match self.read_cache(&rule_set_key(url, format)) {
            Ok(entry) => entry,
            Err(SubscriptionError::ReadCache(error)) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let content = self.store.read_blob(&entry.snapshot_hash)?;
        let path = self
            .store
            .cache_path(&rule_set_key(url, format))
            .with_extension("rules");
        if std::fs::read(&path).ok().as_deref() != Some(content.as_slice()) {
            sempre_state::write_atomic(&path, &content, 0o600)
                .map_err(SubscriptionError::WriteCache)?;
        }
        Ok(Some(RuleSetSnapshot {
            path,
            content,
            fetched_at: entry.fetched_at,
        }))
    }

    pub async fn fetch_rule_set(&self, url: &str) -> Result<RuleSetSnapshot, SubscriptionError> {
        let source = Source {
            id: String::new(),
            kind: "url".into(),
            enabled: true,
            url: url.into(),
            remark: String::new(),
            prefix: String::new(),
            content: String::new(),
            user_agent: String::new(),
            extra: serde_json::Map::new(),
        };
        let content = self
            .download(&source, DEFAULT_USER_AGENT, DEFAULT_FETCH_MODE)
            .await?;
        self.rule_set_candidate(content)
    }

    /// Candidates are immutable and invisible to startup until the manager validates them.
    pub fn rule_set_candidate(
        &self,
        content: Vec<u8>,
    ) -> Result<RuleSetSnapshot, SubscriptionError> {
        let hash = self.store.save_blob(&content)?;
        Ok(RuleSetSnapshot {
            path: self.store.blob_path(&hash),
            content,
            fetched_at: Utc::now(),
        })
    }

    pub fn accept_rule_set(
        &self,
        url: &str,
        format: &str,
        snapshot: &RuleSetSnapshot,
    ) -> Result<(), SubscriptionError> {
        let snapshot_hash = self.store.save_blob(&snapshot.content)?;
        let key = rule_set_key(url, format);
        let path = self.store.cache_path(&key).with_extension("rules");
        // Keep a stable path so the running core can watch validated updates.
        if std::fs::read(&path).ok().as_deref() != Some(snapshot.content.as_slice()) {
            sempre_state::write_atomic(&path, &snapshot.content, 0o600)
                .map_err(SubscriptionError::WriteCache)?;
        }
        self.write_cache(
            &key,
            &CacheEntry {
                url: url.into(),
                user_agent: DEFAULT_USER_AGENT.into(),
                fetch_mode: format!("rule-set:{format}"),
                snapshot_hash,
                fetched_at: snapshot.fetched_at,
            },
        )
    }
}

fn rule_set_key(url: &str, format: &str) -> String {
    format!("core-rule-set\0{format}\0{url}")
}

#[cfg(test)]
mod tests;
