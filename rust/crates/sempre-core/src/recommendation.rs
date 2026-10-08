use crate::{
    AutoConfigCandidateProfile, AutoConfigDnsFallback, AutoConfigRelease, AutoConfigValidation,
    BuiltInKind, Target,
};

pub(crate) fn profiles(kind: BuiltInKind, target: &Target) -> Vec<AutoConfigCandidateProfile> {
    if !matches!(target.os.as_str(), "darwin" | "linux" | "windows")
        || !matches!(target.arch.as_str(), "amd64" | "arm64")
    {
        return Vec::new();
    }
    match kind {
        BuiltInKind::SingBox if target.os == "darwin" => vec![
            profile(
                "sing-box/macos-native-dns-v14",
                "sing-box@1.14.0-beta.13",
                "macos-tun-native-dns",
                AutoConfigValidation::Compatible,
                AutoConfigRelease::Preview,
                AutoConfigDnsFallback::External,
            ),
            profile(
                "sing-box/macos-stable",
                "sing-box@stable",
                "macos-tun-external-dns",
                AutoConfigValidation::Verified,
                AutoConfigRelease::Stable,
                AutoConfigDnsFallback::External,
            ),
            profile(
                "sing-box/macos-standalone-v12",
                "sing-box@1.12.20",
                "macos-tun-real-ip",
                AutoConfigValidation::Verified,
                AutoConfigRelease::Legacy,
                AutoConfigDnsFallback::DestinationOverride,
            ),
        ],
        BuiltInKind::SingBox if target.os == "windows" => vec![
            profile(
                "sing-box/windows-native-dns-v14",
                "sing-box@1.14.0-beta.13",
                "windows-tun-native-dns",
                AutoConfigValidation::Verified,
                AutoConfigRelease::Preview,
                AutoConfigDnsFallback::External,
            ),
            profile(
                "sing-box/windows-stable",
                "sing-box@stable",
                "platform-tun",
                AutoConfigValidation::Verified,
                AutoConfigRelease::Stable,
                AutoConfigDnsFallback::External,
            ),
        ],
        BuiltInKind::SingBox => vec![profile(
            "sing-box/stable",
            "sing-box@stable",
            "platform-tun",
            AutoConfigValidation::Verified,
            AutoConfigRelease::Stable,
            AutoConfigDnsFallback::External,
        )],
        BuiltInKind::Mihomo => vec![profile(
            "mihomo/stable",
            "mihomo@stable",
            "mihomo-tun",
            if target.os == "darwin" {
                AutoConfigValidation::Compatible
            } else {
                AutoConfigValidation::Verified
            },
            AutoConfigRelease::Stable,
            AutoConfigDnsFallback::External,
        )],
        _ => Vec::new(),
    }
}

fn profile(
    id: &str,
    reference: &str,
    configuration_mode: &str,
    validation: AutoConfigValidation,
    release: AutoConfigRelease,
    dns_fallback: AutoConfigDnsFallback,
) -> AutoConfigCandidateProfile {
    AutoConfigCandidateProfile {
        id: id.into(),
        reference: reference.into(),
        configuration_mode: configuration_mode.into(),
        validation,
        release,
        dns_fallback,
    }
}
