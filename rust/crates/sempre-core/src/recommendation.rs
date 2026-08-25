use crate::{AutoConfigCandidate, BuiltInKind, Target};

pub(crate) fn candidates(kind: BuiltInKind, target: &Target) -> Vec<AutoConfigCandidate> {
    if !matches!(target.os.as_str(), "darwin" | "linux" | "windows")
        || !matches!(target.arch.as_str(), "amd64" | "arm64")
    {
        return Vec::new();
    }
    match kind {
        BuiltInKind::SingBox if target.os == "darwin" => vec![
            candidate(
                "sing-box/macos-standalone-v12",
                "sing-box@1.12.20",
                "macos-tun-real-ip",
                100,
                &["macos-standalone-compatible", "legacy-destination-override"],
                &["legacy-core-version"],
            ),
            candidate(
                "sing-box/macos-stable",
                "sing-box@stable",
                "macos-tun-external-dns",
                55,
                &["stable-release", "broad-protocol-support"],
                &["external-system-dns-required"],
            ),
        ],
        BuiltInKind::SingBox => vec![candidate(
            "sing-box/stable",
            "sing-box@stable",
            "platform-tun",
            100,
            &["stable-release", "broad-protocol-support"],
            &[],
        )],
        BuiltInKind::Mihomo => vec![candidate(
            "mihomo/stable",
            "mihomo@stable",
            "mihomo-tun",
            if target.os == "darwin" { 70 } else { 90 },
            &["stable-release", "broad-protocol-support"],
            if target.os == "darwin" {
                &["not-verified-for-standalone-macos"]
            } else {
                &[]
            },
        )],
        _ => Vec::new(),
    }
}

fn candidate(
    id: &str,
    reference: &str,
    mode: &str,
    score: i32,
    reasons: &[&str],
    warnings: &[&str],
) -> AutoConfigCandidate {
    AutoConfigCandidate {
        id: id.into(),
        core: String::new(),
        reference: reference.into(),
        configuration_mode: mode.into(),
        score,
        reasons: reasons.iter().map(ToString::to_string).collect(),
        warnings: warnings.iter().map(ToString::to_string).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_prefers_the_compatible_sing_box_release() {
        let target = Target {
            os: "darwin".into(),
            arch: "arm64".into(),
            amd64_level: 0,
        };
        let candidates = candidates(BuiltInKind::SingBox, &target);
        assert_eq!(candidates[0].id, "sing-box/macos-standalone-v12");
        assert_eq!(candidates[0].reference, "sing-box@1.12.20");
    }
}
