//! Resolve server-owned remote rule URLs before invoking the pure converter.
use sempre_converter::{EditorConfig, Profile, Target, prepare_profile};
use url::Url;

use crate::error::ApiError;

const GFW_RULES: &str = "https://cdn.jsdelivr.net/gh/ohmywrt/clash-rule@master/gfwip.yaml";

pub(crate) fn prepare(
    profile: &Profile,
    target: &Target,
    public_url: &Url,
) -> Result<Profile, ApiError> {
    let mut profile = prepare_profile(profile, target)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    // All editor fields have now been resolved. Recompilation must not replace
    // the server's remote-rule references with the source YAML URLs.
    profile.editor = EditorConfig::default();
    for provider in &mut profile.rule_providers {
        if provider.url.is_empty() {
            continue;
        }
        let binary = provider.format == "binary"
            || Url::parse(&provider.url).is_ok_and(|url| {
                std::path::Path::new(url.path())
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("srs"))
            });
        if !binary {
            provider.url = rule_url(public_url, &target.version, &provider.url)?;
            provider.format = "source".into();
        }
    }
    profile.dns["shared"]["gfwBlackRuleSetUrl"] =
        rule_url(public_url, &target.version, GFW_RULES)?.into();
    Ok(profile)
}

fn rule_url(base: &Url, version: &str, source: &str) -> Result<String, ApiError> {
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
