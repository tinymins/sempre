use std::fmt;

use thiserror::Error;

pub const STABLE: &str = "stable";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoreRef {
    pub core: String,
    pub repository: Option<String>,
    pub reference: String,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ReferenceError {
    #[error("invalid core reference {0:?}")]
    Reference(String),
    #[error("invalid core name {0:?}")]
    Core(String),
    #[error("invalid GitHub repository {0:?}; expected owner/repository")]
    Repository(String),
    #[error("core reference cannot be empty")]
    Empty,
    #[error("invalid core version or channel {0:?}")]
    Version(String),
}

impl CoreRef {
    pub fn parse(input: &str) -> Result<Self, ReferenceError> {
        let input = input.trim();
        if input.matches('@').count() > 1 {
            return Err(ReferenceError::Reference(input.into()));
        }
        let (source, reference) = input.split_once('@').map_or((input, STABLE), |value| value);
        if source.matches(':').count() > 1 {
            return Err(ReferenceError::Reference(source.into()));
        }
        let (core, repository) = source
            .split_once(':')
            .map_or((source, None), |(core, repository)| {
                (core, Some(repository))
            });
        if !valid_core(core) {
            return Err(ReferenceError::Core(core.into()));
        }
        let repository = repository
            .map(|value| {
                if valid_repository(value) {
                    Ok(value.to_ascii_lowercase())
                } else {
                    Err(ReferenceError::Repository(value.into()))
                }
            })
            .transpose()?;
        if reference.is_empty() {
            return Err(ReferenceError::Empty);
        }
        let reference = reference.strip_prefix('v').unwrap_or(reference);
        if reference != STABLE && !valid_version(reference) {
            return Err(ReferenceError::Version(reference.into()));
        }
        Ok(Self {
            core: core.into(),
            repository,
            reference: reference.into(),
        })
    }

    pub fn is_channel(&self) -> bool {
        self.reference == STABLE
    }
}

impl fmt::Display for CoreRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.core)?;
        if let Some(repository) = &self.repository {
            write!(formatter, ":{repository}")?;
        }
        write!(formatter, "@{}", self.reference)
    }
}

fn valid_core(value: &str) -> bool {
    !value.is_empty()
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_repository(value: &str) -> bool {
    let Some((owner, name)) = value.split_once('/') else {
        return false;
    };
    !owner.is_empty()
        && owner.len() <= 39
        && owner.as_bytes()[0].is_ascii_alphanumeric()
        && owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && !name.is_empty()
        && name.len() <= 100
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
        && !name.contains('/')
}

fn valid_version(value: &str) -> bool {
    let boundary = value.find(['-', '+']);
    let (core, suffix) = boundary.map_or((value, None), |index| {
        (&value[..index], value.get(index + 1..))
    });
    core.split('.').count() == 3
        && core
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && suffix.is_none_or(|suffix| {
            !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_and_custom_sources() {
        assert_eq!(
            CoreRef::parse("sing-box").expect("default").to_string(),
            "sing-box@stable"
        );
        assert_eq!(
            CoreRef::parse("sing-box:SagerNet/Sing-Box@v1.12.0")
                .expect("custom")
                .to_string(),
            "sing-box:sagernet/sing-box@1.12.0"
        );
    }

    #[test]
    fn rejects_path_escape_versions_and_repositories() {
        assert!(CoreRef::parse("sing-box:owner/../repo@stable").is_err());
        assert!(CoreRef::parse("sing-box@1.2.3-../../escape").is_err());
    }
}
