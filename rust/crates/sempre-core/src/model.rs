use std::{collections::BTreeMap, path::PathBuf};

use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Target {
    pub os: String,
    pub arch: String,
    pub amd64_level: u8,
}

impl Target {
    pub fn current() -> Self {
        Self {
            os: match std::env::consts::OS {
                "macos" => "darwin",
                value => value,
            }
            .into(),
            arch: match std::env::consts::ARCH {
                "x86_64" => "amd64",
                "aarch64" => "arm64",
                value => value,
            }
            .into(),
            amd64_level: current_amd64_level(),
        }
    }

    pub fn platform(&self) -> String {
        format!("{}/{}", self.os, self.arch)
    }
}

#[cfg(target_arch = "x86_64")]
fn current_amd64_level() -> u8 {
    if std::arch::is_x86_feature_detected!("avx2")
        && std::arch::is_x86_feature_detected!("bmi1")
        && std::arch::is_x86_feature_detected!("bmi2")
        && std::arch::is_x86_feature_detected!("f16c")
        && std::arch::is_x86_feature_detected!("fma")
        && std::arch::is_x86_feature_detected!("lzcnt")
        && std::arch::is_x86_feature_detected!("movbe")
        && std::arch::is_x86_feature_detected!("xsave")
    {
        3
    } else if std::arch::is_x86_feature_detected!("sse3")
        && std::arch::is_x86_feature_detected!("ssse3")
        && std::arch::is_x86_feature_detected!("sse4.1")
        && std::arch::is_x86_feature_detected!("sse4.2")
        && std::arch::is_x86_feature_detected!("popcnt")
    {
        2
    } else {
        1
    }
}

#[cfg(not(target_arch = "x86_64"))]
const fn current_amd64_level() -> u8 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_target_uses_product_platform_names() {
        let target = Target::current();
        assert!(!matches!(target.os.as_str(), "macos"));
        assert!(!matches!(target.arch.as_str(), "x86_64" | "aarch64"));
        if target.arch == "amd64" {
            assert!((1..=3).contains(&target.amd64_level));
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Package {
    pub version: String,
    pub name: String,
    pub url: String,
    pub digest: String,
    pub size: u64,
    pub format: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetSelection {
    pub names: Vec<String>,
    pub format: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub working_directory: Option<PathBuf>,
}

pub type RunSpec = CommandSpec;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ControlSpec {
    pub core: String,
    pub protocol: ControlProtocol,
    pub base_url: String,
    #[serde(skip_serializing)]
    pub secret: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControlProtocol {
    ClashRest,
    Grpc,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeSpec {
    pub config: PathBuf,
    pub control: Option<ControlSpec>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CompilerTarget {
    pub format: String,
    pub version: Option<String>,
    pub platform: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stability {
    Stable,
    Experimental,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Definition {
    pub id: String,
    pub name: String,
    pub stability: Stability,
    pub compiler_format: String,
    pub control_protocol: Option<ControlProtocol>,
    pub platforms: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutoConfigScore {
    pub id: String,
    pub points: u16,
    pub maximum: u16,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AutoConfigRequirements {
    pub private_dns: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutoConfigCandidate {
    pub id: String,
    pub core: String,
    pub reference: String,
    pub configuration_mode: String,
    pub eligible: bool,
    pub score: Option<u16>,
    pub score_breakdown: Vec<AutoConfigScore>,
    pub reasons: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<String>,
}
