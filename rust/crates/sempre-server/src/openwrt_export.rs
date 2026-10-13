//! Resolve server-owned remote rule URLs before invoking the pure converter.
use sempre_converter::{EditorConfig, Profile, Target, prepare_profile};
use url::Url;

use crate::{
    error::ApiError,
    resource_export::{relay_url, rule_url},
};

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
        if binary {
            provider.url = relay_url(public_url, &provider.url)?;
            provider.format = "binary".into();
        } else if provider.format == "source" {
            provider.url = relay_url(public_url, &provider.url)?;
        } else {
            provider.url = rule_url(public_url, &target.version, &provider.url)?;
            provider.format = "source".into();
        }
    }
    for (prefix, fallback) in [
        (
            "cnDomain",
            "https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-cn.srs",
        ),
        (
            "cnIp",
            "https://cdn.jsdelivr.net/gh/SagerNet/sing-geoip@rule-set/geoip-cn.srs",
        ),
        (
            "hkIp",
            "https://cdn.jsdelivr.net/gh/SagerNet/sing-geoip@rule-set/geoip-hk.srs",
        ),
    ] {
        let key = format!("{prefix}RuleSetUrl");
        let source = profile.dns["shared"][&key].as_str().unwrap_or(fallback);
        let wrapped = relay_url(public_url, source)?;
        profile.dns["shared"][key] = wrapped.into();
        profile.dns["shared"][format!("{prefix}RuleSetDetour")] = "direct".into();
    }
    profile.dns["shared"]["gfwBlackRuleSetUrl"] =
        rule_url(public_url, &target.version, GFW_RULES)?.into();
    Ok(profile)
}
