use std::{fs, fs::File, io::Write as _, path::Path};

use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::{BuildError, checksum};

pub fn prepare_ui(source: &Path, destination: &Path, version: &str) -> Result<String, BuildError> {
    if version.trim().is_empty() {
        return Err(BuildError::invalid("UI version cannot be empty"));
    }
    let temporary = tempfile::tempdir()
        .map_err(|error| BuildError::io("create UI staging directory", source, error))?;
    copy_tree(source, temporary.path())?;
    let manifest_path = temporary.path().join(sempre_ui::MANIFEST_NAME);
    let data = fs::read(&manifest_path)
        .map_err(|error| BuildError::io("read UI manifest", &manifest_path, error))?;
    let mut manifest: sempre_ui::Manifest =
        serde_json::from_slice(&data).map_err(|source| BuildError::Decode {
            name: "UI manifest",
            source,
        })?;
    manifest.version = version.into();
    let mut data = serde_json::to_vec_pretty(&manifest).map_err(|source| BuildError::Decode {
        name: "UI manifest",
        source,
    })?;
    data.push(b'\n');
    sempre_state::write_atomic(&manifest_path, &data, 0o644)
        .map_err(|error| BuildError::io("write UI manifest", &manifest_path, error))?;
    zip_directory(temporary.path(), destination)?;
    checksum::sha256(destination)
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), BuildError> {
    let metadata = source
        .symlink_metadata()
        .map_err(|error| BuildError::io("inspect UI directory", source, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(BuildError::invalid(format!(
            "UI source must be a directory without symbolic links: {}",
            source.display()
        )));
    }
    fs::create_dir_all(target)
        .map_err(|error| BuildError::io("create UI directory", target, error))?;
    for entry in
        fs::read_dir(source).map_err(|error| BuildError::io("read UI directory", source, error))?
    {
        let entry = entry.map_err(|error| BuildError::io("read UI entry", source, error))?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let kind = entry
            .file_type()
            .map_err(|error| BuildError::io("inspect UI entry", &from, error))?;
        if kind.is_symlink() {
            return Err(BuildError::invalid(format!(
                "UI source contains a symbolic link: {}",
                from.display()
            )));
        }
        if kind.is_dir() {
            copy_tree(&from, &to)?;
        } else if kind.is_file() {
            fs::copy(&from, &to).map_err(|error| BuildError::io("copy UI file", &to, error))?;
        }
    }
    Ok(())
}

fn zip_directory(source: &Path, destination: &Path) -> Result<(), BuildError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| BuildError::io("create UI archive directory", parent, error))?;
    }
    let file = File::create(destination)
        .map_err(|error| BuildError::io("create UI archive", destination, error))?;
    let mut archive = ZipWriter::new(file);
    append_directory(&mut archive, source, source)?;
    archive
        .finish()
        .map_err(|error| BuildError::invalid(format!("finish UI archive: {error}")))?;
    Ok(())
}

fn append_directory(
    archive: &mut ZipWriter<File>,
    root: &Path,
    directory: &Path,
) -> Result<(), BuildError> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| BuildError::io("read UI archive directory", directory, error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| BuildError::io("read UI archive entry", directory, error))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| BuildError::invalid("UI archive path escaped its root"))?;
        let name = relative.to_string_lossy().replace('\\', "/");
        let kind = entry
            .file_type()
            .map_err(|error| BuildError::io("inspect UI archive entry", &path, error))?;
        if kind.is_dir() {
            archive
                .add_directory(
                    format!("{name}/"),
                    SimpleFileOptions::default().unix_permissions(0o755),
                )
                .map_err(|error| BuildError::invalid(format!("archive UI directory: {error}")))?;
            append_directory(archive, root, &path)?;
        } else if kind.is_file() {
            archive
                .start_file(
                    name,
                    SimpleFileOptions::default()
                        .compression_method(CompressionMethod::Deflated)
                        .unix_permissions(0o644),
                )
                .map_err(|error| BuildError::invalid(format!("archive UI file: {error}")))?;
            let data = fs::read(&path)
                .map_err(|error| BuildError::io("read UI archive file", &path, error))?;
            archive.write_all(&data).map_err(|error| {
                BuildError::io("write UI archive file", destination_path(root), error)
            })?;
        }
    }
    Ok(())
}

fn destination_path(root: &Path) -> &Path {
    root
}

#[cfg(test)]
mod tests {
    use std::io::Read as _;

    use super::*;

    #[test]
    fn stamps_and_archives_the_ui_manifest() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = temporary.path().join("source");
        fs::create_dir(&source).expect("source");
        fs::write(source.join("index.html"), "UI").expect("entry");
        fs::write(
            source.join(sempre_ui::MANIFEST_NAME),
            r#"{"schema":1,"name":"Sempre UI","version":"dev","entry":"index.html","api":{"major":1}}"#,
        )
        .expect("manifest");
        let archive_path = temporary.path().join("sempre-ui.zip");
        let digest = prepare_ui(&source, &archive_path, "v2.0.0").expect("UI archive");
        assert_eq!(digest.len(), 64);
        let mut archive =
            zip::ZipArchive::new(File::open(archive_path).expect("archive")).expect("ZIP");
        let mut manifest = String::new();
        archive
            .by_name(sempre_ui::MANIFEST_NAME)
            .expect("manifest entry")
            .read_to_string(&mut manifest)
            .expect("manifest text");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&manifest).expect("JSON")["version"],
            "v2.0.0"
        );
    }
}
