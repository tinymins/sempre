use crate::{AutoConfigCandidate, AutoConfigRequirements, AutoConfigScore, BuiltInKind, Target};

const PLATFORM_MAXIMUM: u16 = 30;
const RELEASE_MAXIMUM: u16 = 25;
const DNS_MAXIMUM: u16 = 30;
const PROTOCOL_MAXIMUM: u16 = 15;

#[derive(Clone, Copy)]
enum Confidence {
    Verified,
    Unverified,
}

#[derive(Clone, Copy)]
enum Release {
    Stable,
    Preview,
    Compatibility,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum DnsIntegration {
    Native,
    External,
    LegacySniff,
}

#[derive(Clone, Copy)]
struct CandidateDefinition {
    id: &'static str,
    reference: &'static str,
    mode: &'static str,
    confidence: Confidence,
    release: Release,
    dns: DnsIntegration,
}

pub(crate) fn candidates(
    kind: BuiltInKind,
    target: &Target,
    requirements: AutoConfigRequirements,
) -> Vec<AutoConfigCandidate> {
    if !matches!(target.os.as_str(), "darwin" | "linux" | "windows")
        || !matches!(target.arch.as_str(), "amd64" | "arm64")
    {
        return Vec::new();
    }
    definitions(kind, target)
        .into_iter()
        .map(|definition| evaluate(definition, target, requirements))
        .collect()
}

fn definitions(kind: BuiltInKind, target: &Target) -> Vec<CandidateDefinition> {
    match kind {
        BuiltInKind::SingBox if target.os == "darwin" => vec![
            CandidateDefinition {
                id: "sing-box/macos-native-dns-v14",
                reference: "sing-box@1.14.0-beta.13",
                mode: "macos-tun-native-dns",
                confidence: Confidence::Verified,
                release: Release::Preview,
                dns: DnsIntegration::Native,
            },
            CandidateDefinition {
                id: "sing-box/macos-stable",
                reference: "sing-box@stable",
                mode: "macos-tun-external-dns",
                confidence: Confidence::Verified,
                release: Release::Stable,
                dns: DnsIntegration::External,
            },
            CandidateDefinition {
                id: "sing-box/macos-standalone-v12",
                reference: "sing-box@1.12.20",
                mode: "macos-tun-real-ip",
                confidence: Confidence::Verified,
                release: Release::Compatibility,
                dns: DnsIntegration::LegacySniff,
            },
        ],
        BuiltInKind::SingBox => vec![CandidateDefinition {
            id: "sing-box/stable",
            reference: "sing-box@stable",
            mode: "platform-tun",
            confidence: Confidence::Verified,
            release: Release::Stable,
            dns: DnsIntegration::Native,
        }],
        BuiltInKind::Mihomo => vec![CandidateDefinition {
            id: "mihomo/stable",
            reference: "mihomo@stable",
            mode: "mihomo-tun",
            confidence: if target.os == "darwin" {
                Confidence::Unverified
            } else {
                Confidence::Verified
            },
            release: Release::Stable,
            dns: if target.os == "darwin" {
                DnsIntegration::External
            } else {
                DnsIntegration::Native
            },
        }],
        _ => Vec::new(),
    }
}

fn evaluate(
    definition: CandidateDefinition,
    target: &Target,
    requirements: AutoConfigRequirements,
) -> AutoConfigCandidate {
    let mut reasons = vec![match definition.confidence {
        Confidence::Verified => "platform-verified".into(),
        Confidence::Unverified => "platform-compatible".into(),
    }];
    let mut warnings = Vec::new();
    match definition.release {
        Release::Stable => reasons.push("stable-release".into()),
        Release::Preview => warnings.push("preview-release".into()),
        Release::Compatibility => warnings.push("legacy-core-version".into()),
    }
    match definition.dns {
        DnsIntegration::Native => reasons.push("native-dns-integration".into()),
        DnsIntegration::External => warnings.push("external-system-dns-required".into()),
        DnsIntegration::LegacySniff => reasons.push("legacy-destination-override".into()),
    }
    if matches!(definition.confidence, Confidence::Unverified) {
        warnings.push("not-verified-for-standalone-macos".into());
    }
    reasons.push("broad-protocol-support".into());

    let mut blockers = Vec::new();
    if target.os == "darwin" && requirements.private_dns && definition.dns != DnsIntegration::Native
    {
        blockers.push("private-dns-requires-native-integration".into());
    }
    if requirements.private_dns && definition.dns == DnsIntegration::Native {
        reasons.push("private-dns-compatible".into());
    }
    let eligible = blockers.is_empty();
    let score_breakdown = if eligible {
        score_breakdown(definition)
    } else {
        Vec::new()
    };
    let score = eligible.then(|| score_breakdown.iter().map(|item| item.points).sum());
    AutoConfigCandidate {
        id: definition.id.into(),
        core: String::new(),
        reference: definition.reference.into(),
        configuration_mode: definition.mode.into(),
        eligible,
        score,
        score_breakdown,
        reasons,
        warnings,
        blockers,
    }
}

fn score_breakdown(definition: CandidateDefinition) -> Vec<AutoConfigScore> {
    vec![
        score(
            "platform",
            match definition.confidence {
                Confidence::Verified => PLATFORM_MAXIMUM,
                Confidence::Unverified => 15,
            },
            PLATFORM_MAXIMUM,
        ),
        score(
            "release",
            match definition.release {
                Release::Stable => RELEASE_MAXIMUM,
                Release::Preview => 10,
                Release::Compatibility => 15,
            },
            RELEASE_MAXIMUM,
        ),
        score(
            "dns",
            match definition.dns {
                DnsIntegration::Native => DNS_MAXIMUM,
                DnsIntegration::External => 22,
                DnsIntegration::LegacySniff => 12,
            },
            DNS_MAXIMUM,
        ),
        score("protocols", PROTOCOL_MAXIMUM, PROTOCOL_MAXIMUM),
    ]
}

fn score(id: &str, points: u16, maximum: u16) -> AutoConfigScore {
    AutoConfigScore {
        id: id.into(),
        points,
        maximum,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn macos() -> Target {
        Target {
            os: "darwin".into(),
            arch: "arm64".into(),
            amd64_level: 0,
        }
    }

    #[test]
    fn stable_release_scores_highest_without_private_dns() {
        let candidates = candidates(
            BuiltInKind::SingBox,
            &macos(),
            AutoConfigRequirements::default(),
        );
        let stable = candidates
            .iter()
            .find(|candidate| candidate.id == "sing-box/macos-stable")
            .expect("stable candidate");
        let preview = candidates
            .iter()
            .find(|candidate| candidate.id == "sing-box/macos-native-dns-v14")
            .expect("preview candidate");
        assert!(stable.score > preview.score);
        assert!(stable.eligible && preview.eligible);
    }

    #[test]
    fn private_dns_requires_native_macos_integration() {
        let candidates = candidates(
            BuiltInKind::SingBox,
            &macos(),
            AutoConfigRequirements { private_dns: true },
        );
        let native = candidates
            .iter()
            .find(|candidate| candidate.id == "sing-box/macos-native-dns-v14")
            .expect("native candidate");
        assert!(native.eligible);
        assert!(native.score.is_some());
        for candidate in candidates
            .iter()
            .filter(|candidate| candidate.id != native.id)
        {
            assert!(!candidate.eligible);
            assert_eq!(candidate.score, None);
            assert_eq!(
                candidate.blockers,
                ["private-dns-requires-native-integration"]
            );
        }
    }
}
