use futures_util::{StreamExt as _, stream};
use serde_json::json;
use sha2::{Digest, Sha256};

use sempre_converter::{
    Diagnostic, Profile, SourceSnapshot, profile_from_editor, rule_provider_snapshot_id,
};

use crate::{
    AppState,
    debug_stream::StageLog,
    error::ApiError,
    source_cache::{self, CacheMode, LoadedSource, SourceKind, SourceRequest},
};

pub(crate) async fn load_rule_source(
    state: &AppState,
    url: &str,
    ua: &str,
    source_id: &str,
    ttl_minutes: i32,
    cache_mode: CacheMode,
) -> Result<LoadedSource, ApiError> {
    if let Some(proxy) = state.config.direct_proxy_url.as_deref() {
        let direct = source_cache::load(
            state,
            SourceRequest {
                url,
                ua,
                fetch_mode: "domestic-direct",
                proxy: Some(proxy),
                source_id,
                ttl_minutes,
                mode: cache_mode,
                kind: SourceKind::RuleSet,
                inspect_unusable: false,
            },
        );
        let automatic = source_cache::load(
            state,
            SourceRequest {
                url,
                ua,
                fetch_mode: "auto",
                proxy: None,
                source_id,
                ttl_minutes,
                mode: cache_mode,
                kind: SourceKind::RuleSet,
                inspect_unusable: false,
            },
        );
        tokio::pin!(direct, automatic);
        return tokio::select! {
            result = &mut direct => match result {
                Ok(loaded) => Ok(loaded),
                Err(error) => {
                    tracing::warn!(reason = %error.message(), "configured rule fetch route failed");
                    automatic.await
                }
            },
            result = &mut automatic => match result {
                Ok(loaded) => Ok(loaded),
                Err(error) => {
                    tracing::warn!(reason = %error.message(), "automatic rule fetch route failed");
                    direct.await
                }
            },
        };
    }
    source_cache::load(
        state,
        SourceRequest {
            url,
            ua,
            fetch_mode: "auto",
            proxy: None,
            source_id,
            ttl_minutes,
            mode: cache_mode,
            kind: SourceKind::RuleSet,
            inspect_unusable: false,
        },
    )
    .await
}

pub(crate) async fn load_rule_snapshots(
    state: &AppState,
    profile: &Profile,
    cache_mode: CacheMode,
    ttl_minutes: i32,
    snapshots: &mut Vec<SourceSnapshot>,
    diagnostics: &mut Vec<Diagnostic>,
    stages: &mut StageLog,
) -> Result<(), ApiError> {
    let effective =
        profile_from_editor(profile).map_err(|error| ApiError::bad_request(error.to_string()))?;
    let providers = effective
        .rule_providers
        .into_iter()
        .filter(|provider| !provider.url.trim().is_empty())
        .collect::<Vec<_>>();
    if !providers.is_empty() {
        stages.push(json!({"type":"rule-providers","status":"running","count":providers.len()}));
    }
    let loaded_rules = stream::iter(providers.into_iter().map(|provider| async move {
        let source_id = rule_provider_snapshot_id(&provider.tag);
        let loaded = load_rule_source(
            state,
            &provider.url,
            "clash.meta",
            &source_id,
            ttl_minutes,
            cache_mode,
        )
        .await;
        (source_id, loaded)
    }))
    .buffered(4);
    tokio::pin!(loaded_rules);
    while let Some((source_id, loaded)) = loaded_rules.next().await {
        let loaded = loaded.inspect_err(|error| {
            stages.push(json!({"type":"rule-provider","status":"error","sourceId":source_id,"message":error.message()}));
        })?;
        stages.push(json!({
            "type":"rule-provider","status":"ok","sourceId":source_id,
            "cached":matches!(loaded.cache_state, "fresh" | "stale"),
            "cacheState":loaded.cache_state,"message":loaded.warning.clone(),
        }));
        if let Some(message) = loaded.warning {
            diagnostics.push(Diagnostic {
                level: "warning".into(),
                source_id: Some(source_id.clone()),
                message,
            });
        }
        snapshots.push(SourceSnapshot {
            source_id,
            content_hash: format!("{:x}", Sha256::digest(loaded.content.as_bytes())),
            content: loaded.content,
        });
    }
    Ok(())
}
