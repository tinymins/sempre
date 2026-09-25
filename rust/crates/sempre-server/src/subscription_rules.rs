use futures_util::{StreamExt as _, stream};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use sempre_converter::{
    Diagnostic, Profile, SourceSnapshot, profile_from_editor, rule_provider_snapshot_id,
};

use crate::{
    AppState,
    error::ApiError,
    source_cache::{self, CacheMode, SourceKind, SourceRequest},
};

pub(crate) async fn load_rule_snapshots(
    state: &AppState,
    profile: &Profile,
    cache_mode: CacheMode,
    ttl_minutes: i32,
    snapshots: &mut Vec<SourceSnapshot>,
    diagnostics: &mut Vec<Diagnostic>,
    stages: &mut Vec<Value>,
) -> Result<(), ApiError> {
    let effective =
        profile_from_editor(profile).map_err(|error| ApiError::bad_request(error.to_string()))?;
    let providers = effective
        .rule_providers
        .into_iter()
        .filter(|provider| !provider.url.trim().is_empty());
    let loaded_rules = stream::iter(providers.map(|provider| async move {
        let source_id = rule_provider_snapshot_id(&provider.tag);
        let loaded = source_cache::load(
            state,
            SourceRequest {
                url: &provider.url,
                ua: "clash.meta",
                fetch_mode: "auto",
                proxy: None,
                source_id: &source_id,
                ttl_minutes,
                mode: cache_mode,
                kind: SourceKind::RuleSet,
            },
        )
        .await;
        (source_id, loaded)
    }))
    .buffered(4)
    .collect::<Vec<_>>()
    .await;
    for (source_id, loaded) in loaded_rules {
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
