use std::{
    fs,
    io::{self, Write},
    path::Path,
};

#[cfg(unix)]
use std::fs::File;

pub fn write_atomic(path: &Path, data: &[u8], mode: u32) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "atomic path has no parent"))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".sempre-write-")
        .tempfile_in(parent)?;
    temporary.write_all(data)?;
    set_permissions(temporary.path(), mode)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    sync_parent(parent)
}

#[cfg(unix)]
fn set_permissions(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn set_permissions(_: &Path, _: u32) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> io::Result<()> {
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn sync_parent(_: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_a_file_without_leaving_temporary_data() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("state.json");
        write_atomic(&path, b"first", 0o600).expect("first write");
        write_atomic(&path, b"second", 0o600).expect("replacement write");
        assert_eq!(fs::read(&path).expect("content"), b"second");
        assert_eq!(fs::read_dir(root.path()).expect("directory").count(), 1);
    }
}
