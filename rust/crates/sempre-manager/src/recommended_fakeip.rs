use std::fs;

use ipnet::IpNet;
use sempre_converter::{DnsFrontendPolicy, EditorDefaults, Profile, Target};
use sempre_state::{ConfigBuild, Deployment, Document};
use serde_json::Value;

use crate::{Manager, ManagerError, RuntimePendingChange, ValidationRunner, VersionRunner};

const RANGE_FIELDS: [&str; 2] = ["fakeipIpv4Range", "fakeipIpv6Range"];

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    fn recommended_dns(&self, target: &Target) -> Result<Value, ManagerError> {
        let mut dns = sempre_converter::recommended_defaults_for_target(target).dns;
        if !supports_fakeip(target) {
            return Ok(dns);
        }
        let routes = self
            .fakeip_system_routes()
            .map_err(ManagerError::InvalidOperation)?;
        for field in RANGE_FIELDS {
            if target.core == "clash-rs" && field == RANGE_FIELDS[1] {
                continue;
            }
            let Some(range) = dns
                .pointer(&format!("/shared/{field}"))
                .and_then(Value::as_str)
                .and_then(|range| range.parse::<IpNet>().ok())
            else {
                continue;
            };
            let check = crate::fakeip_selection::check_range(range, &routes);
            if !check.conflicts.is_empty() {
                let recommendation = check.recommendation.ok_or_else(|| {
                    ManagerError::InvalidOperation(format!("no available FakeIP range for {range}"))
                })?;
                dns["shared"][field] = Value::String(recommendation.range);
            }
        }
        Ok(dns)
    }

    pub(crate) fn apply_recommended_fakeip(
        &self,
        profile: &Profile,
        target: &Target,
    ) -> Result<Profile, ManagerError> {
        if uses_recommended_fakeip(profile, target) {
            Ok(with_dns(profile, self.recommended_dns(target)?))
        } else {
            Ok(profile.clone())
        }
    }

    pub(crate) fn apply_runtime_fakeip(
        &self,
        profile: &Profile,
        target: &Target,
        deployment: &Deployment,
    ) -> Result<Profile, ManagerError> {
        if !uses_recommended_fakeip(profile, target)
            || (!self.dns_settings.read().enabled && profile.transparent_proxy.mode == "disabled")
        {
            return Ok(profile.clone());
        }
        let data = fs::read(
            self.store
                .layout()
                .dns_frontend_policy(&deployment.config_hash),
        )
        .map_err(|error| ManagerError::io("read compiled DNS policy", error))?;
        let policy: DnsFrontendPolicy = serde_json::from_slice(&data).map_err(|error| {
            ManagerError::InvalidOperation(format!("decode compiled DNS policy: {error}"))
        })?;
        let mut dns = sempre_converter::recommended_defaults_for_target(target).dns;
        dns["shared"][RANGE_FIELDS[0]] = Value::String(policy.fakeip_ipv4_range);
        dns["shared"][RANGE_FIELDS[1]] = Value::String(policy.fakeip_ipv6_range);
        Ok(with_dns(profile, dns))
    }

    pub(crate) fn rendered_config_build(
        &self,
        rendered: &crate::subscription::RenderedProfile,
    ) -> Result<ConfigBuild, ManagerError> {
        self.subscription_config_build(
            &rendered.updated,
            &rendered.target,
            rendered.dns_frontend_policy.as_ref(),
        )
    }

    pub(crate) fn subscription_config_build(
        &self,
        profile: &Profile,
        target: &Target,
        compiled_policy: Option<&DnsFrontendPolicy>,
    ) -> Result<ConfigBuild, ManagerError> {
        let mut build =
            crate::config_build::config_build(profile, target, &self.dns_settings.read())?;
        if uses_recommended_fakeip(profile, target) {
            let ranges = match compiled_policy {
                Some(policy) => [
                    Value::String(policy.fakeip_ipv4_range.clone()),
                    Value::String(policy.fakeip_ipv6_range.clone()),
                ],
                None => dns_ranges(&self.recommended_dns(target)?),
            };
            let original = sempre_converter::recommended_defaults_for_target(target).dns;
            if ranges != dns_ranges(&original) {
                build.runtime_key = Some(format!(
                    "{}|fakeip:{}",
                    build.runtime_key.unwrap_or_default(),
                    serde_json::to_string(&ranges).expect("FakeIP ranges serialize")
                ));
            }
        }
        Ok(build)
    }

    pub fn recommended_editor_defaults(&self) -> Result<EditorDefaults, ManagerError> {
        let mut defaults = sempre_converter::recommended_editor_defaults();
        for (core, editor) in &mut defaults.by_core {
            let target = Target {
                core: core.clone(),
                ..Target::parse(match core.as_str() {
                    "mihomo" => "clash-meta",
                    "sing-box" => "sing-box-v13",
                    core => core,
                })?
            };
            editor.dns_config = serde_json::to_string_pretty(&self.recommended_dns(&target)?)
                .expect("DNS defaults serialize");
        }
        let target = self
            .subscription_target(&self.store.read()?)
            .ok()
            .map(|(target, _)| target);
        if let Some(target) = target {
            defaults.editor.dns_config =
                serde_json::to_string_pretty(&self.recommended_dns(&target)?)
                    .expect("DNS defaults serialize");
        }
        Ok(defaults)
    }

    pub(crate) fn fakeip_pending_change(
        &self,
        document: &Document,
    ) -> Option<RuntimePendingChange> {
        if !document
            .runtime
            .pid
            .is_some_and(crate::runtime::process_alive)
        {
            return None;
        }
        let catalog = self.subscriptions.read().ok()?;
        let profile = catalog
            .profiles
            .iter()
            .find(|profile| Some(&profile.id) == document.active_profile_id.as_ref())?;
        let (target, _) = self.subscription_target(document).ok()?;
        if !uses_recommended_fakeip(profile, &target) {
            return None;
        }
        let dns = self.recommended_dns(&target).ok()?;
        let frontend = self.dns_frontend.status();
        let running_ranges = if frontend.enabled {
            frontend.fakeip_ranges
        } else {
            let data = fs::read(document.runtime.runtime_config.as_deref()?).ok()?;
            let config = serde_yaml::from_slice::<Value>(&data).ok()?;
            crate::fakeip_routes::runtime_fakeip_ranges(&config)
        };
        let mut current = Vec::new();
        let mut next = Vec::new();
        for range in running_ranges {
            let field = if range.addr().is_ipv4() {
                RANGE_FIELDS[0]
            } else {
                RANGE_FIELDS[1]
            };
            let desired = dns["shared"][field].as_str()?.parse::<IpNet>().ok()?;
            if range != desired {
                current.push(range.to_string());
                next.push(desired.to_string());
            }
        }
        (!current.is_empty()).then_some(RuntimePendingChange::FakeIp { current, next })
    }
}

fn supports_fakeip(target: &Target) -> bool {
    matches!(target.core.as_str(), "sing-box" | "mihomo" | "clash-rs")
}

fn uses_recommended_fakeip(profile: &Profile, target: &Target) -> bool {
    supports_fakeip(target)
        && crate::subscription::profile_mode(profile) != "remote"
        && profile.extra.get("use_system_dns").and_then(Value::as_bool) == Some(true)
}

fn dns_ranges(dns: &Value) -> [Value; 2] {
    RANGE_FIELDS.map(|field| dns["shared"][field].clone())
}

fn with_dns(profile: &Profile, dns: Value) -> Profile {
    let mut profile = profile.clone();
    profile.dns = dns;
    profile.editor.dns_config.clear();
    profile
        .extra
        .insert("use_system_dns".into(), Value::Bool(false));
    profile
}
