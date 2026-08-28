use std::{path::Path, sync::Arc};

use regex::Regex;

use crate::{
    Adapter, AssetSelection, AutoConfigCandidate, Capabilities, CommandSpec, CompilerTarget,
    ControlProtocol, Definition, Registry, RegistryError, RunSpec, Stability, Target,
    builtin_capabilities::capabilities, runtime,
};

const PLATFORMS: [&str; 6] = [
    "darwin/amd64",
    "darwin/arm64",
    "linux/amd64",
    "linux/arm64",
    "windows/amd64",
    "windows/arm64",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltInKind {
    SingBox,
    Mihomo,
    Xray,
    V2Ray,
    ClashRs,
    Dae,
}

#[derive(Clone, Copy, Debug)]
pub struct BuiltInAdapter {
    kind: BuiltInKind,
}

impl BuiltInAdapter {
    pub const fn new(kind: BuiltInKind) -> Self {
        Self { kind }
    }

    fn validate_target(self, target: &Target) -> Result<(), RegistryError> {
        let supported = if self.kind == BuiltInKind::Dae {
            target.os == "linux" && matches!(target.arch.as_str(), "amd64" | "arm64")
        } else {
            matches!(target.os.as_str(), "windows" | "linux" | "darwin")
                && matches!(target.arch.as_str(), "amd64" | "arm64")
        };
        if supported {
            Ok(())
        } else {
            Err(RegistryError::Target {
                core: self.id().into(),
                target: target.platform(),
            })
        }
    }

    fn simple_target(self, format: &str, target: &Target) -> Result<CompilerTarget, RegistryError> {
        self.validate_target(target)?;
        Ok(CompilerTarget {
            format: format.into(),
            version: None,
            platform: "default".into(),
            warnings: vec![],
        })
    }
}

pub fn built_in_registry() -> Registry {
    Registry::new(
        [
            BuiltInKind::SingBox,
            BuiltInKind::Mihomo,
            BuiltInKind::Xray,
            BuiltInKind::V2Ray,
            BuiltInKind::ClashRs,
            BuiltInKind::Dae,
        ]
        .map(|kind| Arc::new(BuiltInAdapter::new(kind)) as Arc<dyn Adapter>),
    )
}

impl Adapter for BuiltInAdapter {
    fn id(&self) -> &'static str {
        match self.kind {
            BuiltInKind::SingBox => "sing-box",
            BuiltInKind::Mihomo => "mihomo",
            BuiltInKind::Xray => "xray",
            BuiltInKind::V2Ray => "v2ray",
            BuiltInKind::ClashRs => "clash-rs",
            BuiltInKind::Dae => "dae",
        }
    }

    fn default_repository(&self) -> &'static str {
        match self.kind {
            BuiltInKind::SingBox => "SagerNet/sing-box",
            BuiltInKind::Mihomo => "MetaCubeX/mihomo",
            BuiltInKind::Xray => "XTLS/Xray-core",
            BuiltInKind::V2Ray => "v2fly/v2ray-core",
            BuiltInKind::ClashRs => "Watfaq/clash-rs",
            BuiltInKind::Dae => "daeuniverse/dae",
        }
    }

    fn definition(&self) -> Definition {
        let (name, stability, format, control, platforms) = match self.kind {
            BuiltInKind::SingBox => (
                "sing-box",
                Stability::Stable,
                "sing-box-v13",
                Some(ControlProtocol::ClashRest),
                PLATFORMS.as_slice(),
            ),
            BuiltInKind::Mihomo => (
                "Mihomo",
                Stability::Stable,
                "clash-meta",
                Some(ControlProtocol::ClashRest),
                PLATFORMS.as_slice(),
            ),
            BuiltInKind::Xray => (
                "Xray-core",
                Stability::Stable,
                "xray",
                Some(ControlProtocol::Grpc),
                PLATFORMS.as_slice(),
            ),
            BuiltInKind::V2Ray => (
                "V2Ray-core",
                Stability::Stable,
                "v2ray",
                Some(ControlProtocol::Grpc),
                PLATFORMS.as_slice(),
            ),
            BuiltInKind::ClashRs => (
                "clash-rs",
                Stability::Experimental,
                "clash-rs",
                Some(ControlProtocol::ClashRest),
                PLATFORMS.as_slice(),
            ),
            BuiltInKind::Dae => (
                "dae",
                Stability::Experimental,
                "dae",
                None::<ControlProtocol>,
                &["linux/amd64", "linux/arm64"][..],
            ),
        };
        Definition {
            id: self.id().into(),
            name: name.into(),
            stability,
            compiler_format: format.into(),
            control_protocol: control,
            platforms: platforms.iter().map(ToString::to_string).collect(),
        }
    }

    fn capabilities(&self, version: Option<&str>, target: &Target) -> Capabilities {
        capabilities(self.kind, version, target).normalize()
    }

    fn executable_name(&self, target: &Target) -> Result<String, RegistryError> {
        self.validate_target(target)?;
        let name = match self.kind {
            BuiltInKind::SingBox => windows_name("sing-box", target),
            BuiltInKind::Mihomo => windows_name("mihomo", target),
            BuiltInKind::Xray => windows_name("xray", target),
            BuiltInKind::V2Ray => windows_name("v2ray", target),
            BuiltInKind::ClashRs => windows_name("clash-rs", target),
            BuiltInKind::Dae => dae_name(target),
        };
        Ok(name)
    }

    fn package_assets(
        &self,
        version: &str,
        target: &Target,
    ) -> Result<AssetSelection, RegistryError> {
        self.validate_target(target)?;
        let selection = match self.kind {
            BuiltInKind::SingBox => {
                let (format, extension) = if target.os == "windows" {
                    ("zip", ".zip")
                } else {
                    ("tar.gz", ".tar.gz")
                };
                AssetSelection {
                    names: vec![format!(
                        "sing-box-{version}-{}-{}{extension}",
                        target.os, target.arch
                    )],
                    format: format.into(),
                }
            }
            BuiltInKind::Mihomo => mihomo_assets(version, target),
            BuiltInKind::Xray | BuiltInKind::V2Ray => {
                let prefix = if self.kind == BuiltInKind::Xray {
                    "Xray"
                } else {
                    "v2ray"
                };
                let os = if target.os == "darwin" {
                    "macos"
                } else {
                    &target.os
                };
                let arch = if target.arch == "amd64" {
                    "64"
                } else {
                    "arm64-v8a"
                };
                AssetSelection {
                    names: vec![format!("{prefix}-{os}-{arch}.zip")],
                    format: "zip".into(),
                }
            }
            BuiltInKind::ClashRs => AssetSelection {
                names: vec![clash_rs_asset(target)],
                format: "raw".into(),
            },
            BuiltInKind::Dae => AssetSelection {
                names: vec![format!("{}.zip", dae_name(target))],
                format: "zip".into(),
            },
        };
        Ok(selection)
    }

    fn version_command(&self, binary: &str) -> CommandSpec {
        command(
            binary,
            match self.kind {
                BuiltInKind::SingBox | BuiltInKind::Xray | BuiltInKind::V2Ray => &["version"],
                BuiltInKind::Mihomo => &["-v"],
                BuiltInKind::ClashRs | BuiltInKind::Dae => &["--version"],
            },
            None,
        )
    }

    fn parse_version(&self, output: &str) -> Result<String, RegistryError> {
        let pattern = match self.kind {
            BuiltInKind::SingBox => {
                r"(?m)^sing-box version ([0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?)"
            }
            BuiltInKind::Mihomo => {
                r"(?m)^Mihomo Meta v?([0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?)(?:\s|$)"
            }
            BuiltInKind::Xray => r"(?m)^Xray\s+v?([0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?)\b",
            BuiltInKind::V2Ray => {
                r"(?m)^V2Ray\s+v?([0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?)\b"
            }
            BuiltInKind::ClashRs => {
                r"(?mi)^clash-rs\s+v?([0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?)\b"
            }
            BuiltInKind::Dae => {
                r"(?m)^dae version v?([0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?)\s*$"
            }
        };
        Regex::new(pattern)
            .expect("static version pattern")
            .captures(output.trim())
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().into())
            .ok_or_else(|| RegistryError::VersionOutput {
                core: self.id().into(),
                output: output.trim().into(),
            })
    }

    fn compiler_target(
        &self,
        version: Option<&str>,
        target: &Target,
    ) -> Result<CompilerTarget, RegistryError> {
        match self.kind {
            BuiltInKind::SingBox => Ok(sing_box_target(version, target)),
            BuiltInKind::Mihomo => self.simple_target("clash-meta", target),
            BuiltInKind::Xray => self.simple_target("xray", target),
            BuiltInKind::V2Ray => self.simple_target("v2ray", target),
            BuiltInKind::ClashRs => self.simple_target("clash-rs", target),
            BuiltInKind::Dae => self.simple_target("dae", target),
        }
    }

    fn validation_command(&self, binary: &str, config: &str, data: &str) -> CommandSpec {
        let arguments: Vec<&str> = match self.kind {
            BuiltInKind::SingBox => vec!["check", "-c", config, "-D", data, "--disable-color"],
            BuiltInKind::Mihomo => vec!["-t", "-f", config, "-d", data],
            BuiltInKind::Xray => vec!["run", "-test", "-config", config],
            BuiltInKind::V2Ray => vec!["test", "-c", config],
            BuiltInKind::ClashRs => vec![
                "--compatibility",
                "--test-config",
                "--config",
                config,
                "--directory",
                data,
            ],
            BuiltInKind::Dae => vec!["validate", "--config", config],
        };
        let working_directory = match self.kind {
            BuiltInKind::Xray | BuiltInKind::V2Ray | BuiltInKind::Dae => Some(data),
            _ => None,
        };
        with_runtime_environment(self.kind, command(binary, &arguments, working_directory))
    }

    fn run_spec(&self, binary: &str, config: &str, data: &str) -> RunSpec {
        let arguments: Vec<&str> = match self.kind {
            BuiltInKind::SingBox => vec!["run", "-c", config, "-D", data, "--disable-color"],
            BuiltInKind::Mihomo => vec!["-f", config, "-d", data],
            BuiltInKind::Xray => vec!["run", "-config", config],
            BuiltInKind::V2Ray => vec!["run", "-c", config],
            BuiltInKind::ClashRs => {
                vec!["--compatibility", "--config", config, "--directory", data]
            }
            BuiltInKind::Dae => vec![
                "run",
                "--config",
                config,
                "--disable-sudo",
                "--disable-pidfile",
            ],
        };
        with_runtime_environment(self.kind, command(binary, &arguments, Some(data)))
    }

    fn prepare_runtime(
        &self,
        config: &Path,
        runtime_directory: &Path,
    ) -> Result<crate::RuntimeSpec, RegistryError> {
        runtime::prepare(self.kind, config, runtime_directory)
    }

    fn auto_config_candidates(
        &self,
        target: &Target,
        requirements: crate::AutoConfigRequirements,
    ) -> Vec<AutoConfigCandidate> {
        crate::recommendation::candidates(self.kind, target, requirements)
    }
}

fn command(binary: &str, arguments: &[&str], data: Option<&str>) -> CommandSpec {
    CommandSpec {
        program: binary.into(),
        arguments: arguments.iter().map(ToString::to_string).collect(),
        working_directory: data.map(Into::into),
        ..CommandSpec::default()
    }
}

fn with_runtime_environment(kind: BuiltInKind, mut command: CommandSpec) -> CommandSpec {
    let directory = command
        .program
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .display()
        .to_string();
    match kind {
        BuiltInKind::Xray => {
            command
                .environment
                .insert("xray.location.asset".into(), directory);
        }
        BuiltInKind::V2Ray => {
            command
                .environment
                .insert("v2ray.location.asset".into(), directory);
        }
        BuiltInKind::Dae => {
            command
                .environment
                .insert("DAE_LOCATION_ASSET".into(), directory);
        }
        _ => {}
    }
    command
}

fn windows_name(name: &str, target: &Target) -> String {
    if target.os == "windows" {
        format!("{name}.exe")
    } else {
        name.into()
    }
}

fn dae_name(target: &Target) -> String {
    let arch = match (target.arch.as_str(), target.amd64_level) {
        ("amd64", 3..) => "x86_64_v3_avx2",
        ("amd64", 2) => "x86_64_v2_sse",
        ("amd64", _) => "x86_64",
        _ => "arm64",
    };
    format!("dae-linux-{arch}")
}

fn clash_rs_asset(target: &Target) -> String {
    let arch = if target.arch == "amd64" {
        "x86_64"
    } else {
        "aarch64"
    };
    let platform = match target.os.as_str() {
        "darwin" => "apple-darwin",
        "linux" => "unknown-linux-gnu",
        _ => "pc-windows-msvc",
    };
    format!(
        "clash-rs-{arch}-{platform}{}",
        if target.os == "windows" { ".exe" } else { "" }
    )
}

fn mihomo_assets(version: &str, target: &Target) -> AssetSelection {
    let extension = if target.os == "windows" {
        ".zip"
    } else {
        ".gz"
    };
    let format = if target.os == "windows" { "zip" } else { "gz" };
    let prefix = format!("mihomo-{}-{}", target.os, target.arch);
    let names = if target.arch == "arm64" {
        vec![format!("{prefix}-v{version}{extension}")]
    } else {
        let variants = match target.amd64_level {
            3.. => vec!["v3", "v2", "compatible"],
            2 => vec!["v2", "compatible"],
            _ => vec!["compatible"],
        };
        variants
            .into_iter()
            .map(|variant| format!("{prefix}-{variant}-v{version}{extension}"))
            .collect()
    };
    AssetSelection {
        names,
        format: format.into(),
    }
}

fn sing_box_target(version: Option<&str>, target: &Target) -> CompilerTarget {
    let (version, warnings) = resolve_sing_box_version(version.unwrap_or_default());
    let platform = match target.os.as_str() {
        "windows" => "windows",
        "darwin" => "macos",
        _ => "default",
    };
    let mut format = if version == "11" {
        "sing-box".into()
    } else {
        format!("sing-box-v{version}")
    };
    if platform != "default" {
        format.push('-');
        format.push_str(platform);
    }
    CompilerTarget {
        format,
        version: Some(version.into()),
        platform: platform.into(),
        warnings,
    }
}

pub(super) fn resolve_sing_box_version(core_version: &str) -> (&'static str, Vec<String>) {
    let mut parts = core_version.trim_start_matches('v').split('.');
    let major = parts.next().and_then(|value| value.parse::<u32>().ok());
    let minor = parts.next().and_then(|value| value.parse::<u32>().ok());
    match (major, minor) {
        (Some(1), Some(0..=10)) => ("11", vec!["installed sing-box is older than the minimum compiler target; using v11".into()]),
        (Some(1), Some(11)) => ("11", vec![]),
        (Some(1), Some(12)) => ("12", vec![]),
        (Some(1), Some(13)) => ("13", vec![]),
        (Some(1), Some(14)) => ("14", vec![]),
        (Some(1), Some(15..)) => ("14", vec!["no exact compiler for this sing-box minor version; using the newest compatible v14 compiler".into()]),
        (Some(_), Some(_)) => ("13", vec!["unknown sing-box major version; using the default v13 compiler".into()]),
        _ => ("13", vec!["unrecognized sing-box version; using the default v13 compiler".into()]),
    }
}
