use serde::{Deserialize, Serialize};

use crate::CompileError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    #[serde(default)]
    pub core: String,
    pub format: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub standalone: bool,
}

impl Target {
    pub fn parse(format: &str) -> Result<Self, CompileError> {
        let mut target = Self {
            core: String::new(),
            format: format.into(),
            version: String::new(),
            platform: "default".into(),
            standalone: false,
        };
        match format {
            "clash" | "clash-meta" => return Ok(target),
            "clash-rs" | "xray" | "v2ray" | "dae" => {
                target.core = format.into();
                return Ok(target);
            }
            _ => {}
        }
        target.core = "sing-box".into();
        let mut value = format;
        if let Some(stripped) = value.strip_suffix("-windows") {
            value = stripped;
            target.platform = "windows".into();
        } else if let Some(stripped) = value.strip_suffix("-macos") {
            value = stripped;
            target.platform = "macos".into();
        }
        target.version = match value {
            "sing-box" => "11",
            "sing-box-v12" => "12",
            "sing-box-v13" => "13",
            "sing-box-v14" => "14",
            _ => return Err(CompileError::UnsupportedTarget(format.into())),
        }
        .into();
        Ok(target)
    }
}

pub fn available_targets() -> Vec<Target> {
    [
        "clash",
        "clash-meta",
        "sing-box",
        "sing-box-windows",
        "sing-box-macos",
        "sing-box-v12",
        "sing-box-v12-windows",
        "sing-box-v12-macos",
        "sing-box-v13",
        "sing-box-v13-windows",
        "sing-box-v13-macos",
        "sing-box-v14",
        "sing-box-v14-windows",
        "sing-box-v14-macos",
        "xray",
        "v2ray",
        "clash-rs",
        "dae",
    ]
    .into_iter()
    .filter_map(|format| Target::parse(format).ok())
    .collect()
}
