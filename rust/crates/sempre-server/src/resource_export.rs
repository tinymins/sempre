//! Apply server resource URLs after rendering, including generated and custom rules.
use sempre_converter::{CompileRequest, CompileResult, EditorConfig, Profile, prepare_profile};
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;

use crate::error::ApiError;

pub(crate) const REVISION: &str = "resource-relay-export-v2";

pub(crate) fn compile(request: &CompileRequest, base: &Url) -> Result<CompileResult, ApiError> {
    let mut request = request.clone();
    request.profile = prepare_profile(&request.profile, &request.target)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    request.profile.editor = EditorConfig::default();
    if request.target.is_openwrt() {
        request.profile.dns["shared"]["gfwBlackRuleSetUrl"] =
            "https://cdn.jsdelivr.net/gh/ohmywrt/clash-rule@master/gfwip.yaml".into();
    }
    let mut result = sempre_converter::compile(&request)
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
        wrap_sing_box_rules(&mut config, &request.profile, &result.version, base)?;
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
