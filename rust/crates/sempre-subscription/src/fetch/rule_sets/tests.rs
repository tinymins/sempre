use axum::{
    Router,
    http::{HeaderMap, Uri},
    routing::get,
};
use serde_json::json;

use super::*;
use crate::SubscriptionStore;

#[tokio::test]
async fn arbitrary_rule_urls_download_through_the_authenticated_core_proxy_and_remain_cached() {
    let root = tempfile::tempdir().unwrap();
    let store = SubscriptionStore::new(sempre_state::Layout::at(root.path()));
    store.initialize().unwrap();
    let fetcher = Fetcher::new(store).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = "http://unresolvable.invalid/arbitrary-user-rules";
    let rules = "payload:\n  - DOMAIN-SUFFIX,user.example\n";
    let app = Router::new().fallback(get(move |uri: Uri, headers: HeaderMap| async move {
        assert_eq!(uri.host(), Some("unresolvable.invalid"));
        assert_eq!(uri.path(), "/arbitrary-user-rules");
        assert_eq!(headers["proxy-authorization"], "Basic c2VtcHJlOnNlY3JldA==");
        rules
    }));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    assert!(fetcher.cached_rule_set(url, "source").unwrap().is_none());
    let proxy = fetcher
        .via_local_http_proxy(address, "sempre", "secret")
        .unwrap();
    let candidate = proxy.fetch_rule_set(url).await.unwrap();
    assert!(fetcher.cached_rule_set(url, "source").unwrap().is_none());
    fetcher.accept_rule_set(url, "source", &candidate).unwrap();
    server.abort();
    let cached = fetcher.cached_rule_set(url, "source").unwrap().unwrap();
    assert_eq!(cached.content, rules.as_bytes());
    assert_eq!(std::fs::read(&cached.path).unwrap(), rules.as_bytes());
    assert!(fetcher.cached_rule_set(url, "binary").unwrap().is_none());
    // A rejected candidate never replaces the last accepted snapshot.
    let rejected = fetcher
        .rule_set_candidate(b"invalid rules".to_vec())
        .unwrap();
    assert_ne!(rejected.path, cached.path);
    assert_eq!(
        fetcher
            .cached_rule_set(url, "source")
            .unwrap()
            .unwrap()
            .content,
        rules.as_bytes()
    );
    let replacement = fetcher.rule_set_candidate(b"replacement".to_vec()).unwrap();
    fetcher
        .accept_rule_set(url, "source", &replacement)
        .unwrap();
    let updated = fetcher.cached_rule_set(url, "source").unwrap().unwrap();
    assert_eq!(updated.path, cached.path);
    assert_eq!(std::fs::read(&updated.path).unwrap(), b"replacement");
    // Immutable cache corruption must not become a usable startup snapshot.
    std::fs::write(&replacement.path, "corrupt").unwrap();
    assert!(fetcher.cached_rule_set(url, "source").is_err());
}

#[test]
fn startup_reuses_expired_rule_provider_snapshots_without_fetching() {
    let root = tempfile::tempdir().unwrap();
    let store = SubscriptionStore::new(sempre_state::Layout::at(root.path()));
    store.initialize().unwrap();
    let fetcher = Fetcher::new(store).unwrap();
    let source: Source = serde_json::from_value(
        json!({"id":"custom","type":"url","url":"https://offline.invalid/rules"}),
    )
    .unwrap();
    let hash = fetcher
        .store
        .save_blob(b"payload:\n  - DOMAIN,cached.example\n")
        .unwrap();
    fetcher
        .write_cache(
            &format!("{}\0{DEFAULT_USER_AGENT}\0{DEFAULT_FETCH_MODE}", source.url),
            &CacheEntry {
                url: source.url.clone(),
                user_agent: DEFAULT_USER_AGENT.into(),
                fetch_mode: DEFAULT_FETCH_MODE.into(),
                snapshot_hash: hash,
                fetched_at: Utc::now() - chrono::Duration::days(30),
            },
        )
        .unwrap();
    let cached = fetcher.cached_rule_provider(source).unwrap();
    assert!(cached.from_cache);
    assert!(cached.snapshot.content.contains("cached.example"));
}
