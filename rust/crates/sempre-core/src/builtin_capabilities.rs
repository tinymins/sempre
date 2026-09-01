use std::collections::BTreeMap;

use crate::{BuiltInKind, Capabilities, ProtocolCapability, Target, features as f};

pub(super) fn capabilities(
    kind: BuiltInKind,
    version: Option<&str>,
    target: &Target,
) -> Capabilities {
    match kind {
        BuiltInKind::SingBox => sing_box(version, target),
        BuiltInKind::Mihomo => mihomo(target),
        BuiltInKind::Xray => v2ray_family(true, target),
        BuiltInKind::V2Ray => v2ray_family(false, target),
        BuiltInKind::ClashRs => clash_rs(target),
        BuiltInKind::Dae => dae(target),
    }
}

fn sing_box(version: Option<&str>, target: &Target) -> Capabilities {
    let compiler = super::builtins::resolve_sing_box_version(version.unwrap_or_default()).0;
    let mut features = strings(&[
        f::LOGGING_LEVEL,
        f::DNS_LOCAL_UPSTREAM,
        f::DNS_LOCAL_TRANSPORT,
        f::DNS_GEO_SOURCES,
        f::DNS_REMOTE_UPSTREAM,
        f::DNS_REMOTE_PORT,
        f::DNS_BOOTSTRAP_UPSTREAM,
        f::DNS_BOOTSTRAP_PORT,
        f::DNS_BOOTSTRAP_SERVER_NAME,
        f::DNS_REMOTE_SERVER_NAME,
        f::DNS_REMOTE_DETOUR,
        f::DNS_REJECT_HTTPS,
        f::DNS_SPLIT,
        f::DNS_PREFER_IPV4,
        f::ROUTING_RULES,
        f::ROUTING_RULE_PROVIDERS,
        f::ROUTING_SELECTOR,
        f::ROUTING_URL_TEST,
        f::LOCAL_PROXY,
        f::TRANSPARENT_TUN,
        f::TRANSPARENT_TUN_ADDRESS,
        f::MANAGEMENT_CONNECTIONS,
        f::MANAGEMENT_SELECTORS,
        f::MANAGEMENT_DELAY,
        f::MANAGEMENT_TRAFFIC,
        f::MANAGEMENT_EXTERNAL_API,
    ]);
    if target.os != "darwin" || compiler != "11" {
        features.push(f::DNS_FAKE_IP.into());
    }
    if !matches!(target.os.as_str(), "darwin" | "windows") || compiler == "14" {
        features.push(f::DNS_TUN_CAPTURE.into());
    }
    if target.os == "linux"
        || target.os == "darwin"
        || target.os.is_empty()
        || (target.os == "windows" && compiler == "14")
    {
        features.push(f::DNS_SYSTEM_TAKEOVER.into());
    }
    if target.os == "linux" || target.os.is_empty() {
        features.extend(strings(&[f::TRANSPARENT_TPROXY, f::TRANSPARENT_INTERFACES]));
    }
    if compiler != "11" {
        features.push(f::PRIVATE_ACCESS.into());
    }
    Capabilities {
        features,
        enum_values: enums(true, false),
        protocols: vec![
            protocol("http", &["tcp"], &["none", "tls"]),
            protocol("socks5", &["tcp", "udp"], &["none"]),
            protocol("vmess", &["tcp", "ws", "http", "grpc"], &["none", "tls"]),
            protocol(
                "vless",
                &["tcp", "ws", "http", "grpc"],
                &["none", "tls", "reality"],
            ),
            protocol("shadowsocks", &["tcp", "udp"], &["cipher"]),
            protocol("shadowtls", &["tcp"], &["tls"]),
            protocol("trojan", &["tcp", "ws", "grpc"], &["tls", "reality"]),
            protocol("hysteria", &["udp"], &["tls"]),
            protocol("hysteria2", &["udp"], &["tls"]),
            protocol("tuic", &["udp"], &["tls"]),
            ProtocolCapability {
                minimum_version: Some("1.12.0".into()),
                ..protocol("anytls", &["tcp"], &["tls"])
            },
        ],
    }
}

fn mihomo(target: &Target) -> Capabilities {
    let mut features = clash_features();
    features.push(f::TRANSPARENT_TUN.into());
    if target.os != "darwin" {
        features.push(f::DNS_TUN_CAPTURE.into());
    }
    if target.os == "linux" || target.os.is_empty() {
        features.extend(strings(&[f::TRANSPARENT_TPROXY, f::TRANSPARENT_INTERFACES]));
    }
    Capabilities {
        features,
        enum_values: enums(true, true),
        protocols: vec![
            protocol("http", &["tcp"], &["none", "tls"]),
            protocol("socks5", &["tcp", "udp"], &["none"]),
            protocol("vmess", &["tcp", "ws", "http", "grpc"], &["none", "tls"]),
            protocol(
                "vless",
                &["tcp", "ws", "http", "grpc"],
                &["none", "tls", "reality"],
            ),
            protocol("shadowsocks", &["tcp", "udp"], &["cipher"]),
            protocol("shadowtls", &["tcp"], &["tls"]),
            protocol("trojan", &["tcp", "ws", "grpc"], &["tls", "reality"]),
            protocol("hysteria", &["udp"], &["tls"]),
            protocol("hysteria2", &["udp"], &["tls"]),
            protocol("tuic", &["udp"], &["tls"]),
            protocol("anytls", &["tcp"], &["tls"]),
        ],
    }
}

fn clash_rs(target: &Target) -> Capabilities {
    let mut features = clash_features();
    if target.os != "windows" {
        features.extend(strings(&[f::TRANSPARENT_TUN, f::TRANSPARENT_TUN_ADDRESS]));
    }
    if target.os == "linux" || target.os.is_empty() {
        features.push(f::TRANSPARENT_TPROXY.into());
    }
    Capabilities {
        features,
        enum_values: enums(false, true),
        protocols: vec![
            protocol("anytls", &["tcp"], &["tls"]),
            protocol("hysteria2", &["udp"], &["tls"]),
            protocol("shadowsocks", &["tcp", "udp"], &["cipher"]),
            protocol("socks5", &["tcp", "udp"], &["none"]),
            protocol("trojan", &["tcp", "ws", "grpc"], &["tls"]),
            protocol("tuic", &["udp"], &["tls"]),
            protocol(
                "vless",
                &["tcp", "ws", "http", "grpc"],
                &["none", "tls", "reality"],
            ),
            protocol("vmess", &["tcp", "ws", "http", "grpc"], &["none", "tls"]),
        ],
    }
}

fn v2ray_family(xray: bool, target: &Target) -> Capabilities {
    let mut features = strings(&[
        f::LOGGING_LEVEL,
        f::DNS_LOCAL_UPSTREAM,
        f::DNS_REMOTE_UPSTREAM,
        f::DNS_BOOTSTRAP_UPSTREAM,
        f::DNS_PREFER_IPV4,
        f::DNS_REMOTE_SERVER_NAME,
        f::DNS_SPLIT,
        f::DNS_NATIVE,
        f::ROUTING_RULES,
        f::ROUTING_SELECTOR,
        f::ROUTING_URL_TEST,
        f::LOCAL_PROXY,
        f::NATIVE_OVERRIDE,
    ]);
    if xray {
        features.extend(strings(&[f::TRANSPARENT_TUN, f::TRANSPARENT_TUN_ADDRESS]));
    }
    if target.os == "linux" || target.os.is_empty() {
        features.push(f::TRANSPARENT_TPROXY.into());
    }
    let reality = if xray {
        vec!["tls", "reality"]
    } else {
        vec!["tls"]
    };
    Capabilities {
        features,
        enum_values: BTreeMap::from([(
            "proxy_group.type".into(),
            strings(&["select", "url-test"]),
        )]),
        protocols: vec![
            protocol("http", &["tcp"], &["none", "tls"]),
            protocol("shadowsocks", &["tcp", "udp"], &["cipher"]),
            protocol("socks5", &["tcp", "udp"], &["none"]),
            protocol("trojan", &["tcp", "ws", "grpc"], &reality),
            protocol(
                "vless",
                &["tcp", "ws", "http", "grpc"],
                if xray {
                    &["none", "tls", "reality"]
                } else {
                    &["none", "tls"]
                },
            ),
            protocol("vmess", &["tcp", "ws", "http", "grpc"], &["none", "tls"]),
        ],
    }
}

fn dae(target: &Target) -> Capabilities {
    let features = if target.os == "linux" || target.os.is_empty() {
        strings(&[
            f::LOGGING_LEVEL,
            f::DNS_LOCAL_UPSTREAM,
            f::DNS_REMOTE_UPSTREAM,
            f::DNS_REMOTE_PORT,
            f::DNS_BOOTSTRAP_UPSTREAM,
            f::DNS_PREFER_IPV4,
            f::DNS_SPLIT,
            f::ROUTING_RULES,
            f::ROUTING_SELECTOR,
            f::ROUTING_URL_TEST,
            f::TRANSPARENT_EBPF,
            f::NATIVE_OVERRIDE,
        ])
    } else {
        vec![]
    };
    Capabilities {
        features,
        enum_values: BTreeMap::from([(
            "proxy_group.type".into(),
            strings(&["select", "url-test"]),
        )]),
        protocols: vec![
            protocol("anytls", &["tcp"], &["tls"]),
            protocol("http", &["tcp"], &["none", "tls"]),
            protocol("hysteria2", &["udp"], &["tls"]),
            protocol("shadowsocks", &["tcp", "udp"], &["cipher"]),
            protocol("socks5", &["tcp", "udp"], &["none"]),
            protocol("trojan", &["tcp", "ws", "grpc"], &["tls"]),
            protocol("tuic", &["udp"], &["tls"]),
            protocol("vless", &["tcp", "ws", "grpc"], &["none", "tls", "reality"]),
            protocol("vmess", &["tcp", "ws", "grpc"], &["none", "tls"]),
        ],
    }
}

fn clash_features() -> Vec<String> {
    strings(&[
        f::LOGGING_LEVEL,
        f::DNS_LOCAL_UPSTREAM,
        f::DNS_REMOTE_UPSTREAM,
        f::DNS_REMOTE_PORT,
        f::DNS_BOOTSTRAP_UPSTREAM,
        f::DNS_FAKE_IP,
        f::DNS_REMOTE_DETOUR,
        f::DNS_REJECT_HTTPS,
        f::DNS_SPLIT,
        f::DNS_NATIVE,
        f::ROUTING_RULES,
        f::ROUTING_RULE_PROVIDERS,
        f::ROUTING_SELECTOR,
        f::ROUTING_URL_TEST,
        f::LOCAL_PROXY,
        f::MANAGEMENT_CONNECTIONS,
        f::MANAGEMENT_SELECTORS,
        f::MANAGEMENT_DELAY,
        f::MANAGEMENT_TRAFFIC,
        f::MANAGEMENT_EXTERNAL_API,
        f::NATIVE_OVERRIDE,
    ])
}

fn enums(interfaces: bool, mrs: bool) -> BTreeMap<String, Vec<String>> {
    let mut values = BTreeMap::from([
        ("proxy_group.type".into(), strings(&["select", "url-test"])),
        (
            "rule_provider.format".into(),
            strings(if mrs {
                &["yaml", "text", "mrs"]
            } else {
                &["yaml", "text"]
            }),
        ),
    ]);
    if interfaces {
        values.insert(
            "transparent.interface_policy".into(),
            strings(&["all", "include", "exclude"]),
        );
    }
    values
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

fn protocol(name: &str, transports: &[&str], security: &[&str]) -> ProtocolCapability {
    ProtocolCapability {
        protocol: name.into(),
        transports: strings(transports),
        security: strings(security),
        minimum_version: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_dns_capture_is_version_derived() {
        let target = Target {
            os: "darwin".into(),
            arch: "arm64".into(),
            amd64_level: 0,
        };
        assert!(
            capabilities(BuiltInKind::SingBox, Some("1.14.0-beta.13"), &target)
                .features
                .contains(&f::DNS_TUN_CAPTURE.into())
        );
        assert!(
            !capabilities(BuiltInKind::SingBox, Some("1.13.4"), &target)
                .features
                .contains(&f::DNS_TUN_CAPTURE.into())
        );
        let modern = capabilities(BuiltInKind::SingBox, Some("1.13.4"), &target);
        assert!(modern.features.contains(&f::DNS_FAKE_IP.into()));
        assert!(modern.features.contains(&f::DNS_SYSTEM_TAKEOVER.into()));
        assert!(
            !capabilities(BuiltInKind::SingBox, Some("1.11.15"), &target)
                .features
                .contains(&f::DNS_FAKE_IP.into())
        );
    }

    #[test]
    fn windows_dns_capture_requires_sing_box_v14() {
        let target = Target {
            os: "windows".into(),
            arch: "amd64".into(),
            amd64_level: 2,
        };
        let stable = capabilities(BuiltInKind::SingBox, Some("1.13.18"), &target);
        assert!(!stable.features.contains(&f::DNS_TUN_CAPTURE.into()));
        assert!(!stable.features.contains(&f::DNS_SYSTEM_TAKEOVER.into()));
        let v14 = capabilities(BuiltInKind::SingBox, Some("1.14.0-beta.13"), &target);
        assert!(v14.features.contains(&f::DNS_TUN_CAPTURE.into()));
        assert!(v14.features.contains(&f::DNS_SYSTEM_TAKEOVER.into()));
    }
}
