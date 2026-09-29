use super::*;

#[tokio::test]
async fn raw_sources_use_no_network_or_database_and_preserve_order() {
    let url = "postgres://unused:unused@127.0.0.1:1/unused";
    let state = AppState {
        pool: sqlx::postgres::PgPoolOptions::new()
            .connect_lazy(url)
            .unwrap(),
        config: crate::config::Config {
            database_url: url.into(),
            bind_address: "127.0.0.1:0".parse().unwrap(),
            public_url: "http://localhost".parse().unwrap(),
            access_log_retention_days: 1,
            web_root: std::path::PathBuf::default(),
            direct_proxy_url: None,
            trusted_proxy_ips: vec![],
        },
    };
    let fields: SubscriptionFields = serde_json::from_value(json!({"subscribeItems":[
        {"type":"raw","id":"first","content":"proxies:\n  - {name: first, type: socks5, server: 127.0.0.1, port: 1080}","prefix":"A"},
        {"url":"https://unused.invalid","enabled":false},
        {"type":"raw","id":"second","content":"proxies:\n  - {name: second, type: socks5, server: 127.0.0.1, port: 1081}"}
    ]})).unwrap();
    let mut profile = Profile::default();
    let mut snapshots = vec![];
    let mut diagnostics = vec![];
    let mut stages = StageLog::default();
    let summary = load_sources(
        &state,
        &fields,
        &mut profile,
        CacheMode::ReadOnlyGlobal,
        &mut stages,
        &mut snapshots,
        &mut diagnostics,
    )
    .await
    .unwrap();
    assert_eq!(summary.enabled, 2);
    assert_eq!(summary.failed, 0);
    assert_eq!(
        profile
            .sources
            .iter()
            .map(|source| source.id.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(snapshots.len(), 2);
    assert!(
        stages
            .iter()
            .filter(|stage| stage["status"] == "ok")
            .all(|stage| stage["cacheState"] == "inline" && stage["httpStatus"].is_null())
    );
    let request = sempre_converter::CompileRequest {
        protocol: 1,
        profile,
        snapshots,
        custom_nodes: vec![],
        target: sempre_converter::Target::parse("clash-meta").unwrap(),
    };
    let result = sempre_converter::compile(&request).unwrap();
    assert!(result.content.contains("A first"));
    assert!(result.content.contains("second"));
}

#[test]
fn sources_accept_existing_url_records_and_reject_unknown_types() {
    let fields: SubscriptionFields = serde_json::from_value(json!({"subscribeItems":[{"url":"https://example.com"},{"type":"raw","content":"content"}]})).unwrap();
    let items = source_items(&fields).unwrap();
    assert!(items[0].kind == SourceType::Url);
    assert!(items[1].kind == SourceType::Raw);
    let invalid: SubscriptionFields =
        serde_json::from_value(json!({"subscribeItems":[{"type":"other"}]})).unwrap();
    assert!(source_items(&invalid).is_err());
}
