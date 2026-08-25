use sempre_core::Target;

use crate::BuildError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildTarget {
    pub os: String,
    pub arch: String,
}

impl BuildTarget {
    pub fn current() -> Result<Self, BuildError> {
        let target = Target::current();
        Self::new(target.os, target.arch)
    }

    pub fn new(os: impl Into<String>, arch: impl Into<String>) -> Result<Self, BuildError> {
        let target = Self {
            os: os.into(),
            arch: arch.into(),
        };
        if !matches!(target.os.as_str(), "windows" | "linux" | "darwin")
            || !matches!(target.arch.as_str(), "amd64" | "arm64")
        {
            return Err(BuildError::invalid(format!(
                "unsupported release target {}/{}",
                target.os, target.arch
            )));
        }
        Ok(target)
    }

    pub fn core_target(&self) -> Target {
        Target {
            os: self.os.clone(),
            arch: self.arch.clone(),
            amd64_level: u8::from(self.arch == "amd64"),
        }
    }

    pub fn tunnel_target(&self) -> (&'static str, &'static str) {
        let os = match self.os.as_str() {
            "darwin" => "macos",
            "windows" => "windows",
            _ => "linux",
        };
        let arch = if self.arch == "arm64" {
            "aarch64"
        } else {
            "x86_64"
        };
        (os, arch)
    }

    pub fn executable_name(&self) -> &'static str {
        if self.os == "windows" {
            "sempre.exe"
        } else {
            "sempre"
        }
    }

    pub fn binary_name(&self) -> String {
        let suffix = if self.os == "windows" { ".exe" } else { "" };
        format!("sempre-{}-{}{}", self.os, self.arch, suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_product_targets_to_release_tools() {
        let darwin = BuildTarget::new("darwin", "amd64").expect("darwin target");
        assert_eq!(darwin.tunnel_target(), ("macos", "x86_64"));
        assert_eq!(darwin.binary_name(), "sempre-darwin-amd64");
        let windows = BuildTarget::new("windows", "arm64").expect("windows target");
        assert_eq!(windows.tunnel_target(), ("windows", "aarch64"));
        assert_eq!(windows.binary_name(), "sempre-windows-arm64.exe");
        assert!(BuildTarget::new("freebsd", "amd64").is_err());
    }
}
