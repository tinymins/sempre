use std::{fmt, str::FromStr};

use crate::ArtifactError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    pub const fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl FromStr for Sha256Digest {
    type Err = ArtifactError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        let encoded = value
            .get(..7)
            .filter(|prefix| prefix.eq_ignore_ascii_case("sha256:"))
            .and_then(|_| value.get(7..))
            .ok_or_else(|| {
                ArtifactError::invalid("release asset does not provide a SHA-256 digest")
            })?;
        if encoded.len() != 64 {
            return Err(ArtifactError::invalid("invalid SHA-256 digest"));
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in encoded.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = decode_nibble(pair[0])?
                .checked_mul(16)
                .and_then(|high| high.checked_add(decode_nibble(pair[1]).ok()?))
                .ok_or_else(|| ArtifactError::invalid("invalid SHA-256 digest"))?;
        }
        Ok(Self(bytes))
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("sha256:")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

fn decode_nibble(value: u8) -> Result<u8, ArtifactError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(ArtifactError::invalid("invalid SHA-256 digest")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_requires_typed_exact_sha256() {
        let encoded = "Aa".repeat(32);
        let digest: Sha256Digest = format!("SHA256:{encoded}").parse().expect("digest");
        assert_eq!(
            digest.to_string(),
            format!("sha256:{}", encoded.to_lowercase())
        );
        for invalid in [
            "",
            &encoded,
            "sha256:00",
            &format!("sha256:{}g", "0".repeat(63)),
        ] {
            assert!(invalid.parse::<Sha256Digest>().is_err(), "{invalid}");
        }
    }
}
