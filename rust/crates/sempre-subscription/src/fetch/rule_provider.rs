use std::{collections::BTreeMap, fs, io, path::PathBuf};

use sempre_converter::{RuleProvider, rule_provider_has_rules, rule_provider_snapshot_id};
use serde_json::{Map, json};

use super::{
    CacheEntry, DEFAULT_FETCH_MODE, DEFAULT_USER_AGENT, FetchResult, Fetcher, Source,
    SubscriptionError, Utc, result, set_metadata,
};

impl Fetcher {
    pub async fn load_rule_provider(
        &self,
        provider: &RuleProvider,
        refresh: bool,
    ) -> Result<FetchResult, SubscriptionError> {
        let mut source = Source {
            id: rule_provider_snapshot_id(&provider.tag),
            kind: "url".into(),
            enabled: true,
            url: provider.url.clone(),
            remark: provider.tag.clone(),
            prefix: String::new(),
            content: String::new(),
            user_agent: String::new(),
            extra: Map::from_iter([("cache_ttl_minutes".into(), json!(24 * 60))]),
        };
        if refresh {
            return self.load(source, true, validate_rule_provider).await;
        }
        let key = format!(
            "{}\0{DEFAULT_USER_AGENT}\0{DEFAULT_FETCH_MODE}",
            provider.url
        );
        if let Ok(entry) = self.read_cache(&key) {
            set_metadata(&mut source, &entry, "local snapshot", None);
            if let Ok(cached) = self.load_cached(source.clone(), validate_rule_provider) {
                return Ok(cached);
            }
        }
        let bundled = match fs::read(self.store.bundled_rules_path()) {
            Ok(data) => serde_json::from_slice::<BTreeMap<String, String>>(&data)
                .map_err(SubscriptionError::DecodeCache)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(SubscriptionError::ReadCache(error)),
        };
        let content = bundled.get(&provider.url).ok_or_else(|| {
            SubscriptionError::Fetch(format!(
                "rule provider {:?} has no local snapshot; update the subscription first",
                provider.tag
            ))
        })?;
        validate_rule_provider(content)?;
        let hash = self.store.save_blob(content.as_bytes())?;
        let entry = CacheEntry {
            url: source.url.clone(),
            user_agent: DEFAULT_USER_AGENT.into(),
            fetch_mode: DEFAULT_FETCH_MODE.into(),
            snapshot_hash: hash.clone(),
            fetched_at: Utc::now(),
        };
        set_metadata(&mut source, &entry, "bundled snapshot", None);
        Ok(result(source, content, &hash, true))
    }

    pub async fn bundle_system_rule_providers(&self) -> Result<PathBuf, SubscriptionError> {
        let mut bundled = BTreeMap::new();
        for provider in sempre_converter::system_defaults().rule_providers {
            let loaded = self.load_rule_provider(&provider, true).await?;
            bundled.insert(provider.url, loaded.snapshot.content);
        }
        let path = self.store.bundled_rules_path();
        let data = serde_json::to_vec(&bundled).map_err(SubscriptionError::EncodeCache)?;
        sempre_state::write_atomic(&path, &data, 0o600).map_err(SubscriptionError::WriteCache)?;
        Ok(path)
    }
}

fn validate_rule_provider(content: &str) -> Result<(), SubscriptionError> {
    if rule_provider_has_rules(content) {
        Ok(())
    } else {
        Err(SubscriptionError::Invalid(
            "provider has no convertible rules".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::SubscriptionStore;

    use super::*;

    #[tokio::test]
    async fn runtime_uses_bundled_rules_without_contacting_the_provider() {
        let root = tempfile::tempdir().expect("directory");
        let layout = sempre_state::Layout::at(root.path());
        let store = SubscriptionStore::new(layout.clone());
        store.initialize().expect("store");
        fs::create_dir_all(&layout.resources).expect("resources");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/rules", listener.local_addr().unwrap());
        let rules = "payload:\n  - DOMAIN-SUFFIX,example.com\n";
        fs::write(
            store.bundled_rules_path(),
            serde_json::to_vec(&json!({url.clone(): rules})).unwrap(),
        )
        .unwrap();
        let fetcher = Fetcher::new(store).unwrap();
        let provider: RuleProvider =
            serde_json::from_value(json!({"tag":"test", "url":url})).unwrap();
        let loaded = fetcher.load_rule_provider(&provider, false).await.unwrap();
        assert_eq!(loaded.snapshot.content, rules);
        assert_eq!(loaded.source.extra["last_status"], "bundled snapshot");
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn runtime_uses_expired_cache_and_missing_rules_fail_without_downloads() {
        let root = tempfile::tempdir().expect("directory");
        let store = SubscriptionStore::new(sempre_state::Layout::at(root.path()));
        store.initialize().expect("store");
        let fetcher = Fetcher::new(store).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/rules", listener.local_addr().unwrap());
        let provider: RuleProvider =
            serde_json::from_value(json!({"tag":"test", "url":url})).unwrap();
        assert!(fetcher.load_rule_provider(&provider, false).await.is_err());
        let rules = "payload:\n  - DOMAIN-SUFFIX,example.com\n";
        let hash = fetcher.store.save_blob(rules.as_bytes()).unwrap();
        fetcher
            .write_cache(
                &format!("{url}\0{DEFAULT_USER_AGENT}\0{DEFAULT_FETCH_MODE}"),
                &CacheEntry {
                    url: url.clone(),
                    user_agent: DEFAULT_USER_AGENT.into(),
                    fetch_mode: DEFAULT_FETCH_MODE.into(),
                    snapshot_hash: hash,
                    fetched_at: Utc::now() - chrono::Duration::days(30),
                },
            )
            .unwrap();
        let loaded = fetcher.load_rule_provider(&provider, false).await.unwrap();
        assert_eq!(loaded.snapshot.content, rules);
        assert_eq!(loaded.source.extra["last_status"], "local snapshot");
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn explicit_refresh_downloads_and_replaces_the_bundled_snapshot() {
        let root = tempfile::tempdir().expect("directory");
        let store = SubscriptionStore::new(sempre_state::Layout::at(root.path()));
        store.initialize().expect("store");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/rules", listener.local_addr().unwrap());
        let latest = "payload:\n  - DOMAIN-SUFFIX,updated.example\n";
        let app =
            axum::Router::new().route("/rules", axum::routing::get(move || async move { latest }));
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        fs::create_dir_all(store.bundled_rules_path().parent().unwrap()).unwrap();
        fs::write(
            store.bundled_rules_path(),
            serde_json::to_vec(&json!({url.clone(): "payload:\n  - DOMAIN,old.example\n"}))
                .unwrap(),
        )
        .unwrap();
        let fetcher = Fetcher::new(store).unwrap();
        let provider = serde_json::from_value(json!({"tag":"test", "url":url})).unwrap();
        let updated = fetcher.load_rule_provider(&provider, true).await.unwrap();
        assert!(!updated.from_cache);
        assert_eq!(updated.snapshot.content, latest);
        server.abort();
        let cached = fetcher.load_rule_provider(&provider, false).await.unwrap();
        assert_eq!(cached.snapshot.content, latest);
        assert_eq!(cached.source.extra["last_status"], "local snapshot");
    }
}
