use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Value, json};

use crate::{EditorConfig, Profile, ProxyGroup, RuleProvider, Target};

const DIRECT: &str = "🚀 直接连接";
const FOREIGN: &str = "🔰 国外流量";

#[derive(Clone, Debug, Serialize)]
pub struct Defaults {
    pub groups: Vec<ProxyGroup>,
    pub rule_providers: Vec<RuleProvider>,
    pub filters: Vec<String>,
    pub rules: Vec<Value>,
    pub dns: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct EditorDefaults {
    #[serde(flatten)]
    pub editor: EditorConfig,
    pub by_core: BTreeMap<String, EditorConfig>,
}

pub fn system_defaults() -> Defaults {
    Defaults {
        groups: default_groups(),
        rule_providers: default_rule_providers(),
        filters: ["官网", "客服", "qq群"].map(String::from).into(),
        rules: Vec::new(),
        dns: default_dns(),
    }
}

fn default_groups() -> Vec<ProxyGroup> {
    vec![
        group(FOREIGN, &[DIRECT], true, false),
        group("🏳️‍🌈 Google", &[FOREIGN, DIRECT], true, false),
        group("✈️ Telegram", &[FOREIGN, DIRECT], true, false),
        group("🎬 Youtube", &[FOREIGN, DIRECT], true, false),
        group("🎬 TikTok", &[DIRECT, FOREIGN], true, false),
        group("🎬 Netflix", &[FOREIGN, DIRECT], true, false),
        group("🎬 PTTracker", &[DIRECT, FOREIGN], true, false),
        group("👽 Reddit", &[FOREIGN, DIRECT], true, false),
        group("🍎 苹果APNs", &[DIRECT, FOREIGN], true, false),
        group("🍎 苹果服务", &[DIRECT, FOREIGN], true, false),
        group("🪟 Microsoft", &[DIRECT, FOREIGN], true, false),
        group("🎮 Steam", &[FOREIGN, DIRECT], true, false),
        group("🎮 SteamContent", &[DIRECT, FOREIGN], true, false),
        group("🎮 SeasunGame", &[DIRECT, FOREIGN], true, false),
        group("🎮 Discord", &[FOREIGN, DIRECT], true, false),
        group("🤖 ChatGPT-IOS", &[FOREIGN, DIRECT], true, false),
        group("🤖 AI", &[FOREIGN, DIRECT], true, false),
        group("🐙 GitHub", &[FOREIGN, DIRECT], true, false),
        group("🪙 Crypto", &[FOREIGN, DIRECT], true, false),
        group("🛡️ 正版验证拦截", &["REJECT", DIRECT, FOREIGN], true, false),
        group(
            "🧹 秋风广告规则 AWAvenue",
            &[DIRECT, FOREIGN, "REJECT"],
            true,
            false,
        ),
        group(DIRECT, &["DIRECT"], false, true),
        group("💊 广告合集", &["DIRECT", "REJECT"], false, true),
        group("⚓️ 其他流量", &[FOREIGN, DIRECT], false, true),
    ]
}

fn default_rule_providers() -> Vec<RuleProvider> {
    let mut providers = default_rule_providers_primary();
    providers.extend(default_rule_providers_media());
    providers
}

fn default_rule_providers_primary() -> Vec<RuleProvider> {
    vec![
        provider(
            "AppleApns",
            "🍎 苹果APNs",
            "https://raw.githubusercontent.com/ohmywrt/clash-rule/refs/heads/master/AppleAPNs.yaml",
        ),
        provider(
            "Apple",
            "🍎 苹果服务",
            "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Apple.yaml",
        ),
        provider(
            "AppleTV",
            "🍎 苹果服务",
            "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Media/Apple%20TV.yaml",
        ),
        provider(
            "AppleMusic",
            "🍎 苹果服务",
            "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Media/Apple%20Music.yaml",
        ),
        provider(
            "Microsoft",
            "🪟 Microsoft",
            "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Microsoft.yaml",
        ),
        provider(
            "Reddit",
            "👽 Reddit",
            "https://raw.githubusercontent.com/blackmatrix7/ios_rule_script/refs/heads/master/rule/Clash/Reddit/Reddit_No_Resolve.yaml",
        ),
        provider(
            "ChatGPT-IOS",
            "🤖 ChatGPT-IOS",
            "https://raw.githubusercontent.com/ohmywrt/clash-rule/refs/heads/master/chatgpt-ios.yaml",
        ),
        provider(
            "AI",
            "🤖 AI",
            "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/AI%20Suite.yaml",
        ),
        provider(
            "GitHub",
            "🐙 GitHub",
            "https://raw.githubusercontent.com/ohmywrt/clash-rule/refs/heads/master/github.yaml",
        ),
        provider(
            "Crypto",
            "🪙 Crypto",
            "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Crypto.yaml",
        ),
        provider(
            "Youtube",
            "🎬 Youtube",
            "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Media/YouTube.yaml",
        ),
        provider(
            "TikTok",
            "🎬 TikTok",
            "https://raw.githubusercontent.com/Z-Siqi/Clash-for-Windows_Rule/refs/heads/main/Rule/TikTok",
        ),
    ]
}

fn default_rule_providers_media() -> Vec<RuleProvider> {
    vec![
        provider(
            "Netflix",
            "🎬 Netflix",
            "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Media/Netflix.yaml",
        ),
        provider(
            "PTTracker",
            "🎬 PTTracker",
            "https://raw.githubusercontent.com/ohmywrt/clash-rule/refs/heads/master/PTTracker.yaml",
        ),
        provider(
            "Steam",
            "🎮 Steam",
            "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Steam.yaml",
        ),
        provider(
            "SteamContent",
            "🎮 SteamContent",
            "https://raw.githubusercontent.com/ohmywrt/clash-rule/refs/heads/master/SteamContent.yaml",
        ),
        provider(
            "SeasunGame",
            "🎮 SeasunGame",
            "https://raw.githubusercontent.com/ohmywrt/clash-rule/refs/heads/master/SeasunGame.yaml",
        ),
        provider(
            "Discord",
            "🎮 Discord",
            "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Discord.yaml",
        ),
        provider(
            "Telegram",
            "✈️ Telegram",
            "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Telegram.yaml",
        ),
        provider(
            "GoogleCIDRv2",
            "🏳️‍🌈 Google",
            "https://vercel.williamchan.me/api/google-ips",
        ),
        provider(
            "a.dove.is.dumb",
            "🛡️ 正版验证拦截",
            "https://raw.githubusercontent.com/ignaciocastro/a-dove-is-dumb/main/clash.yaml",
        ),
        provider(
            "AWAvenueAD",
            "🧹 秋风广告规则 AWAvenue",
            "https://raw.githubusercontent.com/TG-Twilight/AWAvenue-Ads-Rule/main/Filters/AWAvenue-Ads-Rule-Clash.yaml",
        ),
        provider(
            "AD",
            "💊 广告合集",
            "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/AdBlock.yaml",
        ),
    ]
}

fn default_dns() -> Value {
    json!({ "shared": {
        "localDnsTransport": "udp", "localDns": "223.5.5.5", "localDnsPort": 53, "localServerName": "",
        "bootstrapDns": "223.5.5.5", "bootstrapDnsPort": 853, "bootstrapServerName": "dns.alidns.com",
        "remoteDns": "8.8.8.8", "remoteDnsPort": 853, "remoteServerName": "dns.google",
        "fakeipIpv4Range": "198.18.0.0/15", "fakeipIpv6Range": "fc00::/18",
        "fakeipEnabled": true, "fakeipTtl": 300, "rejectHttps": true,
        "cnDomainLocalDns": true, "cnIpLocalDns": true, "excludeHkFromCnIp": true, "preferIpv4": true,
        "cnDomainRuleSetEnabled": true, "cnDomainRuleSetUrl": "https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-cn.srs", "cnDomainRuleSetDetour": "direct",
        "cnIpRuleSetEnabled": true, "cnIpRuleSetUrl": "https://cdn.jsdelivr.net/gh/SagerNet/sing-geoip@rule-set/geoip-cn.srs", "cnIpRuleSetDetour": "direct",
        "hkIpRuleSetEnabled": true, "hkIpRuleSetUrl": "https://cdn.jsdelivr.net/gh/SagerNet/sing-geoip@rule-set/geoip-hk.srs", "hkIpRuleSetDetour": "direct"
    }})
}

pub fn recommended_defaults(core: &str) -> Defaults {
    let mut defaults = system_defaults();
    if core == "sing-box"
        && let Some(shared) = defaults
            .dns
            .get_mut("shared")
            .and_then(Value::as_object_mut)
    {
        shared.insert("localDnsTransport".into(), json!("tls"));
        shared.insert("localDnsPort".into(), json!(853));
        shared.insert("localServerName".into(), json!("dns.alidns.com"));
    }
    defaults
}

pub fn effective_profile(mut profile: Profile, target: &Target) -> Profile {
    let defaults = recommended_defaults(&target.core);
    if enabled(&profile, "use_system_groups") {
        profile.groups = defaults.groups;
    }
    if enabled(&profile, "use_system_rules") {
        profile.rule_providers = defaults.rule_providers;
    }
    if enabled(&profile, "use_system_filters") {
        profile.filters = defaults.filters;
    }
    if enabled(&profile, "use_system_dns") {
        profile.dns = defaults.dns;
    }
    if enabled(&profile, "use_system_custom_config") {
        profile.rules = defaults.rules;
    }
    profile
}

pub fn recommended_editor_defaults() -> EditorDefaults {
    let editor = editor_config(&system_defaults());
    let by_core = ["sing-box", "mihomo", "clash-rs", "xray", "v2ray", "dae"]
        .into_iter()
        .map(|core| (core.into(), editor_config(&recommended_defaults(core))))
        .collect();
    EditorDefaults { editor, by_core }
}

fn group(name: &str, proxies: &[&str], include_all: bool, readonly: bool) -> ProxyGroup {
    ProxyGroup {
        name: name.into(),
        group_type: "select".into(),
        proxies: proxies.iter().map(|value| (*value).into()).collect(),
        include_all,
        readonly,
        ..ProxyGroup::default()
    }
}

fn provider(tag: &str, outbound: &str, url: &str) -> RuleProvider {
    RuleProvider {
        tag: tag.into(),
        url: url.into(),
        outbound: outbound.into(),
        format: String::new(),
        behavior: String::new(),
        priority: false,
    }
}

fn enabled(profile: &Profile, key: &str) -> bool {
    profile.extra.get(key).and_then(Value::as_bool) == Some(true)
}

fn editor_config(defaults: &Defaults) -> EditorConfig {
    let mut providers = BTreeMap::<String, Vec<Value>>::new();
    for provider in &defaults.rule_providers {
        providers
            .entry(provider.outbound.clone())
            .or_default()
            .push(json!({ "name": provider.tag, "url": provider.url }));
    }
    EditorConfig {
        rule_list: pretty(&providers),
        group: pretty(&defaults.groups),
        filter: pretty(&defaults.filters),
        custom_config: pretty(&defaults.rules),
        dns_config: pretty(&defaults.dns),
        private_access_config: String::new(),
        servers: "[]".into(),
    }
}

fn pretty(value: &impl Serialize) -> String {
    serde_json::to_string_pretty(value).expect("system defaults always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_ohmywrt_policy_shape() {
        let defaults = system_defaults();
        assert_eq!(defaults.groups.len(), 24);
        assert_eq!(defaults.rule_providers.len(), 23);
        assert_eq!(defaults.filters, ["官网", "客服", "qq群"]);
        assert_eq!(defaults.groups[0].name, FOREIGN);
        assert_eq!(defaults.groups[23].name, "⚓️ 其他流量");
        assert_eq!(defaults.rule_providers[0].tag, "AppleApns");
        assert_eq!(defaults.rule_providers[22].tag, "AD");
    }

    #[test]
    fn target_defaults_only_change_the_sing_box_local_dns_transport() {
        let common = system_defaults();
        let sing_box = recommended_defaults("sing-box");
        assert_eq!(common.dns["shared"]["localDnsTransport"], "udp");
        assert_eq!(sing_box.dns["shared"]["localDnsTransport"], "tls");
        assert_eq!(sing_box.dns["shared"]["localDnsPort"], 853);
        assert_eq!(common.groups.len(), sing_box.groups.len());
    }

    #[test]
    fn effective_profile_respects_each_independent_system_switch() {
        let mut profile = Profile::default();
        profile.groups.push(group("custom", &[], false, false));
        profile
            .extra
            .insert("use_system_groups".into(), json!(true));
        let effective = effective_profile(profile, &Target::parse("clash-meta").expect("target"));
        assert_eq!(effective.groups.len(), 24);
        assert!(effective.rule_providers.is_empty());
        assert!(effective.filters.is_empty());
    }

    #[test]
    fn editor_defaults_are_parseable_and_specialized_by_core() {
        let defaults = recommended_editor_defaults();
        assert!(serde_json::from_str::<Value>(&defaults.editor.group).is_ok());
        assert_eq!(defaults.by_core.len(), 6);
        let sing_box: Value = serde_json::from_str(&defaults.by_core["sing-box"].dns_config)
            .expect("sing-box DNS defaults");
        assert_eq!(sing_box["shared"]["localDnsTransport"], "tls");
    }
}
