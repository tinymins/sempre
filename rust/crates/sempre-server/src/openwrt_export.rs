//! Resolve server-owned remote rule URLs before invoking the pure converter.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
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
    base.join(&format!(
        "api/proxy/sing-box/convert/rule/1.{version}/{}",
        URL_SAFE_NO_PAD.encode(source)
    ))
    .map(|url| url.to_string())
    .map_err(ApiError::internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sempre_converter::RuleProvider;
    use serde_json::json;

    #[test]
    fn openwrt_remote_rules_are_versioned_and_query_free() {
        let source = "https://example.com/a%20b.yaml?branch=main&token=test";
        let profile = Profile {
            editor: EditorConfig {
                rule_list:
                    json!({"direct": [{"name": "example", "url": source, "type": "domain"}]})
                        .to_string(),
                dns_config:
                    json!({"shared":{"clashApiSecret":"fixture-secret","tproxyPort":17893}})
                        .to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        for version in ["11", "12", "13", "14"] {
            let format = if version == "11" {
                "sing-box-openwrt".into()
            } else {
                format!("sing-box-v{version}-openwrt")
            };
            let target = Target::parse(&format).unwrap();
            let result = prepare(
                &profile,
                &target,
                &Url::parse("https://server.example/").unwrap(),
            )
            .unwrap();
            let url = Url::parse(&result.rule_providers[0].url).unwrap();
            assert_eq!(url.query(), None);
            assert!(url.path().contains(&format!("/rule/1.{version}/")));
            assert_eq!(
                URL_SAFE_NO_PAD
                    .decode(url.path_segments().unwrap().next_back().unwrap())
                    .unwrap(),
                source.as_bytes()
            );
            assert_eq!(result.transparent_proxy.tproxy.listen_port, 17893);
            assert_eq!(result.management_api.secret, "fixture-secret");
            let again = prepare_profile(&result, &target).unwrap();
            assert_eq!(again.rule_providers[0].url, result.rule_providers[0].url);
            assert_eq!(again.dns, result.dns);
        }
    }

    #[test]
    fn native_binary_rules_keep_their_original_url() {
        let profile = Profile {
            rule_providers: vec![RuleProvider {
                tag: "binary".into(),
                url: "https://example.com/rules.srs".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let target = Target::parse("sing-box-v13-openwrt").unwrap();
        let result = prepare(
            &profile,
            &target,
            &Url::parse("https://server.example/").unwrap(),
        )
        .unwrap();
        assert_eq!(result.rule_providers[0].url, profile.rule_providers[0].url);
    }
}
