use std::{fs, io, net::Ipv4Addr, path::Path};

use crate::TransparentError;

/// Reads a CIDR list, rejecting the whole list on any invalid line so a truncated
/// or hijacked download cannot replace working rules.
pub fn parse_cidrs(text: &str) -> Result<Vec<String>, TransparentError> {
    let mut prefixes = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (address, length) = line.split_once('/').unwrap_or((line, "32"));
        let valid = address.parse::<Ipv4Addr>().is_ok()
            && length.parse::<u8>().is_ok_and(|length| length <= 32);
        if !valid {
            return Err(TransparentError::Invalid(format!(
                "invalid IPv4 prefix {line:?}"
            )));
        }
        prefixes.push(line.to_owned());
    }
    Ok(prefixes)
}

/// A missing list only sends domestic traffic through the core's own routing.
pub(crate) fn load(path: &Path) -> Result<Vec<String>, TransparentError> {
    match fs::read_to_string(path) {
        Ok(text) => parse_cidrs(&text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(TransparentError::Io {
            context: format!("read {}", path.display()),
            source,
        }),
    }
}
