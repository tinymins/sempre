use std::{fs, io, path::PathBuf, process::Command};

use serde_json::{Value, json};

use crate::TransparentError;

const STATE_FILE: &str = "resolv-conf.json";
const MANAGED_CONTENT: &[u8] = b"# Managed by Sempre system DNS takeover. Do not edit while enabled.\nnameserver 127.0.0.1\noptions timeout:1 attempts:1\n";

pub(crate) struct SystemDns {
    allowed: bool,
    state_dir: PathBuf,
    resolv_conf: PathBuf,
}

impl SystemDns {
    pub(crate) fn new(allowed: bool, state_dir: PathBuf, resolv_conf: PathBuf) -> Self {
        Self {
            allowed,
            state_dir,
            resolv_conf,
        }
    }

    pub(crate) fn disabled() -> Self {
        Self::new(false, PathBuf::new(), PathBuf::new())
    }

    pub(crate) const fn allowed(&self) -> bool {
        self.allowed
    }

    pub(crate) fn apply(&self) -> Result<(), TransparentError> {
        if !self.allowed {
            return Err(TransparentError::Invalid(
                "system DNS takeover is only available in Linux system mode".into(),
            ));
        }
        let metadata = fs::symlink_metadata(&self.resolv_conf)
            .map_err(|source| self.io("inspect system resolver", source))?;
        if metadata.file_type().is_symlink() {
            return Err(TransparentError::Invalid(format!(
                "system DNS takeover does not support symlink-managed {}",
                self.resolv_conf.display()
            )));
        }
        let current = fs::read(&self.resolv_conf)
            .map_err(|source| self.io("read system resolver", source))?;
        if current == MANAGED_CONTENT {
            return Ok(());
        }
        fs::create_dir_all(&self.state_dir)
            .map_err(|source| self.io("create system DNS state directory", source))?;
        let state = self.state_path();
        if !state.exists() {
            let mut encoded = serde_json::to_vec_pretty(&json!({
                "original": String::from_utf8_lossy(&current)
            }))
            .map_err(|error| {
                TransparentError::Invalid(format!("encode system DNS backup: {error}"))
            })?;
            encoded.push(b'\n');
            sempre_state::write_atomic(&state, &encoded, 0o600)
                .map_err(|source| self.io("write system DNS backup", source))?;
        }
        sempre_state::write_atomic(&self.resolv_conf, MANAGED_CONTENT, 0o644)
            .map_err(|source| self.io("write system resolver", source))?;
        self.chattr("+i");
        Ok(())
    }

    pub(crate) fn restore(&self) -> Result<(), TransparentError> {
        if self.state_dir.as_os_str().is_empty() {
            return Ok(());
        }
        let state = self.state_path();
        let backup = match fs::read(&state) {
            Ok(backup) => backup,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(source) => return Err(self.io("read system DNS backup", source)),
        };
        let saved: Value = serde_json::from_slice(&backup).map_err(|error| {
            TransparentError::Invalid(format!("decode system DNS backup: {error}"))
        })?;
        let original = saved
            .get("original")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                TransparentError::Invalid(
                    "system DNS backup has no original resolver content".into(),
                )
            })?;
        self.chattr("-i");
        let current = match fs::read(&self.resolv_conf) {
            Ok(current) => Some(current),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(source) => return Err(self.io("read system resolver", source)),
        };
        if current.as_deref().is_none_or(managed) {
            sempre_state::write_atomic(&self.resolv_conf, original.as_bytes(), 0o644)
                .map_err(|source| self.io("restore system resolver", source))?;
        }
        match fs::remove_file(&state) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(self.io("remove system DNS backup", source)),
        }
    }

    pub(crate) fn verify(&self) -> Result<(), TransparentError> {
        if !self.allowed {
            return Ok(());
        }
        let current = fs::read(&self.resolv_conf)
            .map_err(|source| self.io("read system resolver", source))?;
        if managed(&current) {
            Ok(())
        } else {
            Err(TransparentError::Invalid(format!(
                "{} is not managed by Sempre system DNS takeover",
                self.resolv_conf.display()
            )))
        }
    }

    fn state_path(&self) -> PathBuf {
        self.state_dir.join(STATE_FILE)
    }

    fn chattr(&self, flag: &str) {
        if self.resolv_conf == std::path::Path::new("/etc/resolv.conf") {
            let _ = Command::new("chattr")
                .arg(flag)
                .arg(&self.resolv_conf)
                .output();
        }
    }

    fn io(&self, context: &str, source: io::Error) -> TransparentError {
        TransparentError::Io {
            context: format!("{context} {}", self.resolv_conf.display()),
            source,
        }
    }
}

fn managed(data: &[u8]) -> bool {
    data.split(|byte| *byte == b'\n')
        .filter_map(|line| std::str::from_utf8(line).ok())
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .find_map(|line| line.strip_prefix("nameserver").map(str::trim))
        == Some("127.0.0.1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takeover_restores_original_and_preserves_user_replacement() {
        let root = tempfile::tempdir().expect("directory");
        let resolver = root.path().join("resolv.conf");
        let original = b"nameserver 10.251.1.1\nnameserver 223.6.6.6\n";
        fs::write(&resolver, original).expect("resolver");
        let manager = SystemDns::new(true, root.path().join("state"), resolver.clone());
        manager.apply().expect("take over resolver");
        assert!(managed(&fs::read(&resolver).expect("managed resolver")));
        manager.restore().expect("restore resolver");
        assert_eq!(fs::read(&resolver).expect("restored resolver"), original);

        manager.apply().expect("take over again");
        fs::write(&resolver, b"nameserver 9.9.9.9\n").expect("user replacement");
        manager.restore().expect("preserve replacement");
        assert_eq!(
            fs::read(&resolver).expect("user resolver"),
            b"nameserver 9.9.9.9\n"
        );
        assert!(!manager.state_path().exists());
    }

    #[cfg(unix)]
    #[test]
    fn takeover_rejects_symlink_managed_resolver() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().expect("directory");
        let target = root.path().join("target");
        let resolver = root.path().join("resolv.conf");
        fs::write(&target, b"nameserver 1.1.1.1\n").expect("target");
        symlink(target, &resolver).expect("symlink");
        let manager = SystemDns::new(true, root.path().join("state"), resolver);
        assert!(
            manager
                .apply()
                .expect_err("symlink must fail")
                .to_string()
                .contains("symlink-managed")
        );
    }

    #[test]
    fn managed_requires_the_first_nameserver() {
        assert!(managed(
            b"# comment\noptions timeout:1\nnameserver 127.0.0.1\nnameserver 10.0.0.1\n"
        ));
        assert!(!managed(b"nameserver 10.0.0.1\nnameserver 127.0.0.1\n"));
    }
}
