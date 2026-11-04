use sempre_core::Target;

use crate::BuildError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildTarget {
    pub os: String,
    pub arch: String,
    /// Statically linked musl build for `OpenWrt`, packaged with one core at a time.
    pub musl: bool,
}

impl BuildTarget {
    pub fn current() -> Result<Self, BuildError> {
        let target = Target::current();
        Ok(Self {
            musl: cfg!(target_env = "musl"),
            ..Self::new(target.os, target.arch)?
        })
    }

    pub fn new(os: impl Into<String>, arch: impl Into<String>) -> Result<Self, BuildError> {
        let target = Self {
            os: os.into(),
            arch: arch.into(),
            musl: false,
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
        if let Some(arch) = value.strip_prefix("linux-musl-") {
            return Ok(Self {
                musl: true,
                ..Self::new("linux", arch)?
            });
        }
        let (os, arch) = value
            .split_once('-')
            .ok_or_else(|| BuildError::invalid(format!("invalid release target name {value}")))?;
        Self::new(os, arch)
    }

    pub fn ensure_buildable_on(&self, host: &Self) -> Result<(), BuildError> {
        let musl_on_same_linux = self.musl && self.os == host.os && self.arch == host.arch;
        if self == host || musl_on_same_linux || (self.os == "darwin" && host.os == "darwin") {
            return Ok(());
        }
        Err(BuildError::invalid(format!(
            "cannot build {}/{} on {}/{}",
            self.os, self.arch, host.os, host.arch
        )))
    }

    pub fn rust_triple(&self) -> &'static str {
        if self.musl {
            return if self.arch == "arm64" {
                "aarch64-unknown-linux-musl"
            } else {
                "x86_64-unknown-linux-musl"
            };
        }
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
            openwrt: self.musl,
        }
    }

    /// Names a package by the `OpenWrt` architecture its package manager reports.
    pub(crate) fn openwrt_package_name(&self, core: &str, version: &str) -> String {
        let arch = if self.arch == "arm64" {
            "aarch64_generic"
        } else {
            "x86_64"
        };
        format!("sempre-openwrt-{core}-{version}-{arch}")
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
        let libc = if self.musl { "musl-" } else { "" };
        format!("sempre-{}-{libc}{}{suffix}", self.os, self.arch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn musl_targets_resolve_their_triple_and_binary_name() {
        let amd64 = BuildTarget::named("linux-musl-amd64").expect("amd64");
        assert_eq!(amd64.rust_triple(), "x86_64-unknown-linux-musl");
        assert_eq!(amd64.binary_name(), "sempre-linux-musl-amd64");
        let arm64 = BuildTarget::named("linux-musl-arm64").expect("arm64");
        assert_eq!(arm64.rust_triple(), "aarch64-unknown-linux-musl");
        assert_eq!(arm64.binary_name(), "sempre-linux-musl-arm64");
    }

    #[test]
    fn openwrt_packages_use_the_openwrt_architecture_names() {
        let amd64 = BuildTarget::named("linux-musl-amd64").expect("amd64");
        assert_eq!(
            amd64.openwrt_package_name("sing-box", "1.14.4"),
            "sempre-openwrt-sing-box-1.14.4-x86_64"
        );
        assert!(amd64.core_target().openwrt);
        let arm64 = BuildTarget::named("linux-musl-arm64").expect("arm64");
        assert_eq!(
            arm64.openwrt_package_name("sing-box", "1.14.4"),
            "sempre-openwrt-sing-box-1.14.4-aarch64_generic"
        );
    }

    #[test]
    fn existing_target_names_stay_glibc() {
        let target = BuildTarget::named("linux-amd64").expect("target");
        assert!(!target.musl);
        assert_eq!(target.rust_triple(), "x86_64-unknown-linux-gnu");
        assert_eq!(target.binary_name(), "sempre-linux-amd64");
    }

    #[test]
    fn musl_builds_only_on_linux_with_the_same_architecture() {
        let musl = BuildTarget::named("linux-musl-amd64").expect("musl");
        let linux = BuildTarget::named("linux-amd64").expect("linux");
        assert!(musl.ensure_buildable_on(&linux).is_ok());
        assert!(linux.ensure_buildable_on(&musl).is_err());
        assert!(
            musl.ensure_buildable_on(&BuildTarget::named("linux-arm64").expect("arm64"))
                .is_err()
        );
        assert!(
            musl.ensure_buildable_on(&BuildTarget::named("darwin-amd64").expect("darwin"))
                .is_err()
        );
    }

    #[test]
    fn unsupported_musl_architectures_are_rejected() {
        assert!(BuildTarget::named("linux-musl-mips").is_err());
    }
}
