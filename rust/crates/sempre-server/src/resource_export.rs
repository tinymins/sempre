//! Apply server resource URLs after rendering, including generated and custom rules.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use sempre_converter::{
    CompileOverlay, CompileRequest, CompileResult, EditorConfig, Profile, compile_parts,
    prepare_profile,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use crate::{AppState, error::ApiError};

pub(crate) const REVISION: &str = "resource-relay-export-v2";
const CACHE_CAPACITY: usize = 512;

type CompileEntries = HashMap<(Uuid, String), (String, Arc<CompileResult>)>;

/// Remembers the latest compile per subscription target for this server process.
///
/// Process lifetime keeps an upgraded compiler from serving outputs of an older build.
#[derive(Clone, Default)]
pub(crate) struct CompileCache {
    entries: Arc<Mutex<CompileEntries>>,
}

impl CompileCache {
    pub(crate) fn get(
        &self,
        subscription: Uuid,
        target: &str,
        input_hash: &str,
    ) -> Option<Arc<CompileResult>> {
        let entries = self.entries.lock().expect("compile cache");
        entries
            .get(&(subscription, target.to_owned()))
            .filter(|(hash, _)| hash == input_hash)
            .map(|(_, result)| result.clone())
    }

    pub(crate) fn insert(
        &self,
        subscription: Uuid,
        target: &str,
        input_hash: String,
        result: Arc<CompileResult>,
    ) {
        let mut entries = self.entries.lock().expect("compile cache");
        if entries.len() >= CACHE_CAPACITY {
            entries.clear();
        }
        entries.insert((subscription, target.to_owned()), (input_hash, result));
    }
}

/// Reuses the previous output when the compile input is unchanged.
pub(crate) async fn compile_cached(
    state: &AppState,
    subscription: Uuid,
    request: &Arc<CompileRequest>,
    input_hash: &str,
) -> Result<Arc<CompileResult>, ApiError> {
    let target = request.target.format.as_str();
    if let Some(result) = state.compile_cache.get(subscription, target, input_hash) {
        return Ok(result);
    }
    let result =
        Arc::new(compile_blocking(request.clone(), state.config.public_url.clone()).await?);
    state
        .compile_cache
        .insert(subscription, target, input_hash.to_owned(), result.clone());
    Ok(result)
}

/// Runs the CPU-bound compile away from the async workers.
pub(crate) async fn compile_blocking(
    request: Arc<CompileRequest>,
    base: Url,
) -> Result<CompileResult, ApiError> {
    tokio::task::spawn_blocking(move || compile(&request, &base))
        .await
        .map_err(ApiError::internal)?
}

pub(crate) fn compile(request: &CompileRequest, base: &Url) -> Result<CompileResult, ApiError> {
    let mut profile = prepare_profile(&request.profile, &request.target)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    profile.editor = EditorConfig::default();
    if request.target.is_openwrt() {
        profile.dns["shared"]["gfwBlackRuleSetUrl"] =
            sempre_converter::OPENWRT_GFW_BLACK_RULE_SET_URL.into();
    }
    let mut result = compile_parts(
        &profile,
        &request.snapshots,
        &request.custom_nodes,
        &request.target,
        &CompileOverlay::default(),
    )
    .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let clash = matches!(result.format.as_str(), "clash" | "clash-meta" | "clash-rs");
    if !clash && request.target.core != "sing-box" {
        return Ok(result);
    }
    let mut config: Value = if clash {
        serde_yaml::from_str(&result.content).map_err(ApiError::internal)?
    } else {
        serde_json::from_str(&result.content).map_err(ApiError::internal)?
    };
    if clash {
        if let Some(providers) = config
            .get_mut("rule-providers")
            .and_then(Value::as_object_mut)
        {
            for provider in providers.values_mut() {
                wrap_raw_field(provider, "url", base)?;
            }
        }
        if let Some(urls) = config.get_mut("geox-url").and_then(Value::as_object_mut) {
            for value in urls.values_mut() {
                if let Some(source) = value.as_str().filter(|source| remote(source)) {
                    *value = relay_url(base, source)?.into();
                }
            }
        }
    } else {
        wrap_sing_box_rules(&mut config, &profile, &result.version, base)?;
    }
    result.content = if clash {
        serde_yaml::to_string(&config).map_err(ApiError::internal)?
    } else {
        serde_json::to_string_pretty(&config).map_err(ApiError::internal)?
    };
    result.artifact_hash = format!("{:x}", Sha256::digest(result.content.as_bytes()));
    Ok(result)
}

fn remote(source: &str) -> bool {
    Url::parse(source).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn wrap_raw_field(value: &mut Value, key: &str, base: &Url) -> Result<(), ApiError> {
    if let Some(source) = value[key].as_str().filter(|source| remote(source)) {
        value[key] = relay_url(base, source)?.into();
    }
    Ok(())
}

fn wrap_sing_box_rules(
    config: &mut Value,
    profile: &Profile,
    version: &str,
    base: &Url,
) -> Result<(), ApiError> {
    let direct = config
        .get("outbounds")
        .and_then(Value::as_array)
        .and_then(|outbounds| {
            outbounds
                .iter()
                .find(|outbound| outbound["type"] == "direct")
                .and_then(|outbound| outbound["tag"].as_str())
                .map(str::to_owned)
        });
    if let Some(rule_sets) = config
        .get_mut("route")
        .and_then(|route| route.get_mut("rule_set"))
        .and_then(Value::as_array_mut)
    {
        for rule_set in rule_sets {
            if rule_set["type"] != "remote" {
                continue;
            }
            let Some(source) = rule_set["url"].as_str().filter(|source| remote(source)) else {
                continue;
            };
            let provider = profile
                .rule_providers
                .iter()
                .find(|provider| rule_set["tag"] == provider.tag && provider.url == source);
            let binary = rule_set["format"] == "binary"
                || Url::parse(source).is_ok_and(|url| {
                    std::path::Path::new(url.path())
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("srs"))
                });
            let clash_source = provider.is_some_and(|provider| provider.format != "source")
                || (rule_set["tag"] == "geoip-gfwblack"
                    && profile
                        .dns
                        .pointer("/shared/gfwBlackRuleSetUrl")
                        .and_then(Value::as_str)
                        == Some(source));
            let wrapped = if binary || !clash_source {
                relay_url(base, source)?
            } else {
                rule_url(base, version, source)?
            };
            rule_set["url"] = wrapped.into();
            if binary {
                rule_set["format"] = "binary".into();
            } else if clash_source {
                rule_set["format"] = "source".into();
            }
            if let Some(direct) = &direct {
                rule_set["download_detour"] = direct.clone().into();
            }
        }
    }
    Ok(())
}

fn rule_url(base: &Url, version: &str, source: &str) -> Result<String, ApiError> {
    if is_relay_url(base, source) {
        return Ok(source.to_owned());
    }
    let path = match version {
        "11" => "api/proxy/sing-box/convert/rule",
        "12" => "api/proxy/sing-box/convert/rule/12",
        "13" | "14" => "api/proxy/sing-box/convert/rule/13",
        _ => {
            return Err(ApiError::bad_request(
                "unsupported sing-box rule-set version",
            ));
        }
    };
    let mut url = base.join(path).map_err(ApiError::internal)?;
    url.query_pairs_mut().append_pair("url", source);
    Ok(url.to_string())
}

fn relay_url(base: &Url, source: &str) -> Result<String, ApiError> {
    if source.trim().is_empty() || is_relay_url(base, source) {
        return Ok(source.to_owned());
    }
    let mut url = base.join("api/proxy/fetch").map_err(ApiError::internal)?;
    url.query_pairs_mut().append_pair("url", source);
    Ok(url.to_string())
}

fn is_relay_url(base: &Url, source: &str) -> bool {
    let Ok(source) = Url::parse(source) else {
        return false;
    };
    source.origin() == base.origin()
        && base
            .join("api/proxy/")
            .is_ok_and(|prefix| source.path().starts_with(prefix.path()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result() -> Arc<CompileResult> {
        Arc::new(CompileResult {
            protocol: 1,
            format: "sing-box-v13".into(),
            version: "13".into(),
            platform: "default".into(),
            content: "{}".into(),
            artifact_hash: String::new(),
            node_count: 0,
            field_diffs: vec![],
            node_origins: HashMap::new(),
            diagnostics: vec![],
            runtime_validated: false,
        })
    }

    #[test]
    fn cache_only_answers_for_the_same_input() {
        let cache = CompileCache::default();
        let id = Uuid::new_v4();
        cache.insert(id, "sing-box-v13", "first".into(), result());
        assert!(cache.get(id, "sing-box-v13", "first").is_some());
        assert!(cache.get(id, "sing-box-v13", "second").is_none());
        assert!(cache.get(id, "sing-box-v14", "first").is_none());
        assert!(cache.get(Uuid::new_v4(), "sing-box-v13", "first").is_none());
    }

    #[test]
    fn cache_replaces_the_previous_input_for_a_target() {
        let cache = CompileCache::default();
        let id = Uuid::new_v4();
        cache.insert(id, "sing-box-v13", "first".into(), result());
        cache.insert(id, "sing-box-v13", "second".into(), result());
        assert!(cache.get(id, "sing-box-v13", "first").is_none());
        assert!(cache.get(id, "sing-box-v13", "second").is_some());
    }
}
