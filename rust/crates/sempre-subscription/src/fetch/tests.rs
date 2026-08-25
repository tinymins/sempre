use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{Router, routing::get};
use serde_json::{Map, Value};

use super::*;

fn source(kind: &str, url: &str, content: &str) -> Source {
    Source {
        id: "source-1".into(),
        kind: kind.into(),
        enabled: true,
        url: url.into(),
        remark: String::new(),
        prefix: String::new(),
        content: content.into(),
        user_agent: String::new(),
        extra: Map::new(),
    }
}

fn validate(content: &str) -> Result<(), SubscriptionError> {
    if content.contains("server.example") {
        Ok(())
    } else {
        Err(SubscriptionError::Fetch("no usable node".into()))
    }
}

#[test]
fn raw_sources_are_snapshotted_without_network_access() {
    let root = tempfile::tempdir().expect("temporary directory");
    let store = SubscriptionStore::new(sempre_state::Layout::at(root.path()));
    store.initialize().expect("store");
    let fetcher = Fetcher::new(store).expect("fetcher");
    let result = fetcher
        .raw(
            source("raw", "", "ss://secret@server.example:443#node"),
            validate,
        )
        .expect("raw source");
    assert!(!result.from_cache);
    assert_eq!(result.snapshot.content_hash.len(), 64);
    assert_eq!(result.source.extra["last_status"], "raw content");
}

#[tokio::test]
async fn http_sources_use_fresh_and_last_known_good_cache() {
    let requests = Arc::new(AtomicUsize::new(0));
    let counter = requests.clone();
    let app = Router::new().route(
        "/subscription",
        get(move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::Relaxed);
                "ss://secret@server.example:443#node"
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move { axum::serve(listener, app).await });

    let root = tempfile::tempdir().expect("temporary directory");
    let store = SubscriptionStore::new(sempre_state::Layout::at(root.path()));
    store.initialize().expect("store");
    let fetcher = Fetcher::new(store).expect("fetcher");
    let mut input = source("url", &format!("http://{address}/subscription"), "");
    input
        .extra
        .insert("cache_ttl_minutes".into(), Value::from(60));
    let downloaded = fetcher
        .load(input.clone(), false, validate)
        .await
        .expect("download");
    let cached = fetcher
        .load(input.clone(), false, validate)
        .await
        .expect("fresh cache");
    assert!(!downloaded.from_cache && cached.from_cache);
    assert_eq!(requests.load(Ordering::Relaxed), 1);

    server.abort();
    input
        .extra
        .insert("cache_ttl_minutes".into(), Value::from(0));
    let fallback = fetcher
        .load(input, true, validate)
        .await
        .expect("last known good");
    assert!(fallback.from_cache);
    assert_eq!(
        fallback.source.extra["last_status"],
        "last-known-good cache"
    );
    assert!(
        !fallback.source.extra["last_error"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );
}
