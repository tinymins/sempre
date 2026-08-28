use crate::{
    AutoConfigCandidate, AutoConfigCandidateProfile, AutoConfigDnsFallback, AutoConfigRelease,
    AutoConfigRequirements, AutoConfigScore, AutoConfigValidation, Capabilities, features,
};

pub const AUTO_CONFIG_POLICY_VERSION: &str = "constraint-utility-v1";

// The policy uses non-compensatory capability vetoes followed by a fixed-point
// additive utility model. Candidate catalogs provide facts, never scores.
const UTILITY_FULL: u16 = 100;
const UTILITY_HIGH: u16 = 75;
const UTILITY_MEDIUM: u16 = 50;
const UTILITY_LOW: u16 = 25;
const UTILITY_REDUCED: u16 = 40;
const UTILITY_LEGACY: u16 = 60;

const CRITERIA: [Criterion; 4] = [
    Criterion::new("platform", 30, Dimension::PlatformValidation),
    Criterion::new("release", 25, Dimension::ReleaseMaturity),
    Criterion::new("dns", 30, Dimension::DnsIntegrity),
    Criterion::new("protocols", 15, Dimension::ProtocolBreadth),
];
const REFERENCE_PROTOCOLS: [&str; 11] = [
    "anytls",
    "http",
    "hysteria",
    "hysteria2",
    "shadowsocks",
    "shadowtls",
    "socks5",
    "trojan",
    "tuic",
    "vless",
    "vmess",
];

#[derive(Clone, Copy)]
struct Criterion {
    id: &'static str,
    maximum: u16,
    dimension: Dimension,
}

impl Criterion {
    const fn new(id: &'static str, maximum: u16, dimension: Dimension) -> Self {
        Self {
            id,
            maximum,
            dimension,
        }
    }
}

#[derive(Clone, Copy)]
enum Dimension {
    PlatformValidation,
    ReleaseMaturity,
    DnsIntegrity,
    ProtocolBreadth,
}

pub(crate) fn evaluate(
    profile: AutoConfigCandidateProfile,
    core: &str,
    capabilities: &Capabilities,
    requirements: &AutoConfigRequirements,
) -> AutoConfigCandidate {
    let mut matched_requirements = Vec::new();
    let mut blockers = Vec::new();
    for feature in &requirements.required_features {
        if capabilities.features.binary_search(feature).is_ok() {
            matched_requirements.push(format!("feature:{feature}"));
        } else {
            blockers.push(format!("missing-feature:{feature}"));
        }
    }
    for protocol in &requirements.required_protocols {
        if capabilities
            .protocols
            .binary_search_by(|candidate| candidate.protocol.cmp(protocol))
            .is_ok()
        {
            matched_requirements.push(format!("protocol:{protocol}"));
        } else {
            blockers.push(format!("missing-protocol:{protocol}"));
        }
    }

    let native_dns = capabilities
        .features
        .binary_search_by(|feature| feature.as_str().cmp(features::DNS_TUN_CAPTURE))
        .is_ok();
    let eligible = blockers.is_empty();
    let score_breakdown = eligible.then(|| {
        CRITERIA
            .iter()
            .map(|criterion| {
                let utility = utility(
                    criterion.dimension,
                    &profile,
                    native_dns,
                    protocol_coverage(capabilities),
                );
                AutoConfigScore {
                    id: criterion.id.into(),
                    points: weighted_points(criterion.maximum, utility),
                    maximum: criterion.maximum,
                }
            })
            .collect::<Vec<_>>()
    });
    let score = score_breakdown
        .as_ref()
        .map(|items| items.iter().map(|item| item.points).sum());
    let (reasons, warnings) = findings(&profile, native_dns, requirements);

    AutoConfigCandidate {
        id: profile.id,
        core: core.into(),
        reference: profile.reference,
        configuration_mode: profile.configuration_mode,
        eligible,
        score,
        score_breakdown: score_breakdown.unwrap_or_default(),
        matched_requirements,
        reasons,
        warnings,
        blockers,
    }
}

fn utility(
    dimension: Dimension,
    profile: &AutoConfigCandidateProfile,
    native_dns: bool,
    protocol_coverage: u16,
) -> u16 {
    match dimension {
        Dimension::PlatformValidation => match profile.validation {
            AutoConfigValidation::Verified => UTILITY_FULL,
            AutoConfigValidation::Compatible => UTILITY_MEDIUM,
            AutoConfigValidation::Experimental => UTILITY_LOW,
        },
        Dimension::ReleaseMaturity => match profile.release {
            AutoConfigRelease::Stable => UTILITY_FULL,
            AutoConfigRelease::Preview => UTILITY_REDUCED,
            AutoConfigRelease::Legacy => UTILITY_LEGACY,
        },
        Dimension::DnsIntegrity if native_dns => UTILITY_FULL,
        Dimension::DnsIntegrity => match profile.dns_fallback {
            AutoConfigDnsFallback::External => UTILITY_HIGH,
            AutoConfigDnsFallback::DestinationOverride => UTILITY_REDUCED,
        },
        Dimension::ProtocolBreadth => protocol_coverage,
    }
}

fn protocol_coverage(capabilities: &Capabilities) -> u16 {
    let supported = REFERENCE_PROTOCOLS
        .iter()
        .filter(|protocol| {
            capabilities
                .protocols
                .binary_search_by(|candidate| candidate.protocol.as_str().cmp(protocol))
                .is_ok()
        })
        .count();
    u16::try_from(supported * 100 / REFERENCE_PROTOCOLS.len()).unwrap_or(100)
}

fn weighted_points(maximum: u16, utility: u16) -> u16 {
    (maximum * utility + 50) / 100
}

fn findings(
    profile: &AutoConfigCandidateProfile,
    native_dns: bool,
    requirements: &AutoConfigRequirements,
) -> (Vec<String>, Vec<String>) {
    let mut reasons = vec![
        match profile.validation {
            AutoConfigValidation::Verified => "platform-verified",
            AutoConfigValidation::Compatible => "platform-compatible",
            AutoConfigValidation::Experimental => "platform-experimental",
        }
        .into(),
    ];
    let mut warnings = Vec::new();
    match profile.release {
        AutoConfigRelease::Stable => reasons.push("stable-release".into()),
        AutoConfigRelease::Preview => warnings.push("preview-release".into()),
        AutoConfigRelease::Legacy => warnings.push("legacy-core-version".into()),
    }
    if native_dns {
        reasons.push("native-dns-integration".into());
    } else {
        match profile.dns_fallback {
            AutoConfigDnsFallback::External => {
                warnings.push("external-system-dns-required".into());
            }
            AutoConfigDnsFallback::DestinationOverride => {
                reasons.push("legacy-destination-override".into());
            }
        }
    }
    if !requirements.required_features.is_empty() || !requirements.required_protocols.is_empty() {
        reasons.push("requirements-evaluated".into());
    }
    if !matches!(profile.validation, AutoConfigValidation::Verified) {
        warnings.push("not-fully-verified".into());
    }
    (reasons, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProtocolCapability;

    fn profile(validation: AutoConfigValidation) -> AutoConfigCandidateProfile {
        AutoConfigCandidateProfile {
            id: "candidate".into(),
            reference: "core@stable".into(),
            configuration_mode: "tun".into(),
            validation,
            release: AutoConfigRelease::Stable,
            dns_fallback: AutoConfigDnsFallback::External,
        }
    }

    #[test]
    fn policy_weights_total_one_hundred() {
        assert_eq!(
            CRITERIA
                .iter()
                .map(|criterion| criterion.maximum)
                .sum::<u16>(),
            100
        );
    }

    #[test]
    fn missing_required_capability_vetoes_every_score() {
        let mut requirements = AutoConfigRequirements::default();
        requirements.require_feature(features::DNS_TUN_CAPTURE);
        let candidate = evaluate(
            profile(AutoConfigValidation::Verified),
            "core",
            &Capabilities::default(),
            &requirements,
        );
        assert!(!candidate.eligible);
        assert_eq!(candidate.score, None);
        assert!(candidate.score_breakdown.is_empty());
        assert_eq!(
            candidate.blockers,
            [format!("missing-feature:{}", features::DNS_TUN_CAPTURE)]
        );
    }

    #[test]
    fn stronger_evidence_is_monotonic() {
        let requirements = AutoConfigRequirements::default();
        let compatible = evaluate(
            profile(AutoConfigValidation::Compatible),
            "core",
            &Capabilities::default(),
            &requirements,
        );
        let verified = evaluate(
            profile(AutoConfigValidation::Verified),
            "core",
            &Capabilities::default(),
            &requirements,
        );
        assert!(verified.score > compatible.score);
    }

    #[test]
    fn unsupported_required_protocol_is_a_hard_veto() {
        let mut requirements = AutoConfigRequirements::default();
        requirements.require_protocol("future-protocol");
        let candidate = evaluate(
            profile(AutoConfigValidation::Verified),
            "core",
            &Capabilities::default(),
            &requirements,
        );
        assert!(!candidate.eligible);
        assert_eq!(candidate.blockers, ["missing-protocol:future-protocol"]);
    }

    #[test]
    fn protocol_score_uses_a_fixed_policy_benchmark() {
        let unknown = Capabilities {
            protocols: vec![ProtocolCapability {
                protocol: "unknown".into(),
                transports: Vec::new(),
                security: Vec::new(),
                minimum_version: None,
            }],
            ..Capabilities::default()
        }
        .normalize();
        assert_eq!(protocol_coverage(&unknown), 0);
        assert_eq!(protocol_coverage(&Capabilities::default()), 0);
    }
}
