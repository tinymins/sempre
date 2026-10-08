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

    pub fn named(value: &str) -> Result<Self, BuildError> {
        let (os, arch) = value
            .split_once('-')
            .ok_or_else(|| BuildError::invalid(format!("invalid release target name {value}")))?;
        Self::new(os, arch)
    }

    pub fn ensure_buildable_on(&self, host: &Self) -> Result<(), BuildError> {
        if self == host || (self.os == "darwin" && host.os == "darwin") {
            return Ok(());
        }
        Err(BuildError::invalid(format!(
            "cannot build {}/{} on {}/{}",
            self.os, self.arch, host.os, host.arch
        )))
    }

    pub fn rust_triple(&self) -> &'static str {
        match (self.os.as_str(), self.arch.as_str()) {
            ("windows", "amd64") => "x86_64-pc-windows-msvc",
            ("windows", "arm64") => "aarch64-pc-windows-msvc",
            ("linux", "amd64") => "x86_64-unknown-linux-gnu",
            ("linux", "arm64") => "aarch64-unknown-linux-gnu",
            ("darwin", "amd64") => "x86_64-apple-darwin",
            ("darwin", "arm64") => "aarch64-apple-darwin",
            _ => unreachable!("validated release target"),
        }
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
