use std::{fmt::Write as _, fs, io::Read as _, path::Path};

use sha2::{Digest as _, Sha256};

use crate::BuildError;

pub(crate) fn sha256(path: &Path) -> Result<String, BuildError> {
    let mut file =
        fs::File::open(path).map_err(|error| BuildError::io("open checksum input", path, error))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| BuildError::io("read checksum input", path, error))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn write(directory: &Path, names: &[String]) -> Result<(), BuildError> {
    let mut names = names.to_vec();
    names.sort();
    names.dedup();
    let mut content = String::new();
    for name in names {
        if name.contains(['/', '\\']) {
            return Err(BuildError::invalid(format!(
                "checksum entry must be a file name: {name:?}"
            )));
        }
        let digest = sha256(&directory.join(&name))?;
        writeln!(&mut content, "{digest}  {name}").expect("writing to a string cannot fail");
    }
    let path = directory.join("SHA256SUMS");
    sempre_state::write_atomic(&path, content.as_bytes(), 0o644)
        .map_err(|error| BuildError::io("write checksums", path, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_sorted_sha256_entries() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        fs::write(temporary.path().join("b"), b"b").expect("b");
        fs::write(temporary.path().join("a"), b"a").expect("a");
        write(temporary.path(), &["b".into(), "a".into()]).expect("checksums");
        let content =
            fs::read_to_string(temporary.path().join("SHA256SUMS")).expect("checksum file");
        assert_eq!(
            content
                .lines()
                .map(|line| line.rsplit_once(' ').unwrap().1)
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }
}
