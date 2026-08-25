use std::{
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Component, Path, PathBuf},
    time::Duration,
};

use chrono::Utc;
use futures_util::StreamExt as _;
use reqwest::{Client, StatusCode, header};
use sha2::{Digest as _, Sha256};
use tempfile::{Builder, NamedTempFile};
use tokio::io::AsyncWriteExt as _;
use url::Url;

use crate::{
    MANIFEST_NAME, MAX_ARCHIVE_SIZE, MAX_EXPANDED_SIZE, MAX_EXTRACTED_FILES, METADATA_NAME,
    Manifest, Metadata, Store, UiError, validate,
};

impl Store {
    pub fn install_bytes(
        &self,
        data: &[u8],
        source_type: &str,
        source: &str,
        expected_digest: &str,
    ) -> Result<Metadata, UiError> {
        if data.is_empty() || data.len() > MAX_ARCHIVE_SIZE {
            return Err(invalid(format!(
                "UI archive size must be between 1 and {MAX_ARCHIVE_SIZE} bytes"
            )));
        }
        fs::create_dir_all(&self.root).map_err(UiError::Write)?;
        let mut archive = NamedTempFile::new_in(&self.root).map_err(UiError::Write)?;
        archive.write_all(data).map_err(UiError::Write)?;
        archive.flush().map_err(UiError::Write)?;
        self.install_file(archive.path(), source_type, source, expected_digest)
    }

    pub fn install_file(
        &self,
        path: &Path,
        source_type: &str,
        source: &str,
        expected_digest: &str,
    ) -> Result<Metadata, UiError> {
        let metadata = fs::metadata(path).map_err(UiError::Read)?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ARCHIVE_SIZE as u64 {
            return Err(invalid(format!(
                "UI archive size must be between 1 and {MAX_ARCHIVE_SIZE} bytes"
            )));
        }
        let data = fs::read(path).map_err(UiError::Read)?;
        let digest = format!("{:x}", Sha256::digest(&data));
        verify_digest(expected_digest, &digest)?;
        let parent = self.root.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(UiError::Write)?;
        let staging = Builder::new()
            .prefix(".sempre-ui-")
            .tempdir_in(parent)
            .map_err(UiError::Write)?;
        extract(path, staging.path())?;
        let manifest = read_manifest(staging.path())?;
        let installed = Metadata {
            manifest,
            source_type: source_type.into(),
            source: source.into(),
            digest,
            installed_at: Utc::now(),
        };
        write_metadata(staging.path(), &installed)?;
        let staging = staging.keep();
        if let Err(error) = self.activate(&staging) {
            let _ = fs::remove_dir_all(staging);
            return Err(error);
        }
        Ok(installed)
    }

    pub async fn install_url(
        &self,
        value: &str,
        source_type: &str,
        source: &str,
        expected_digest: &str,
    ) -> Result<Metadata, UiError> {
        let url = valid_https_url(value)?;
        fs::create_dir_all(&self.root).map_err(UiError::Write)?;
        let temporary = NamedTempFile::new_in(&self.root).map_err(UiError::Write)?;
        let target = temporary.reopen().map_err(UiError::Write)?;
        let mut target = tokio::fs::File::from_std(target);
        let client = Client::builder()
            .timeout(Duration::from_mins(10))
            .redirect(https_redirect_policy())
            .user_agent(concat!("Sempre/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(UiError::Http)?;
        let response = client
            .get(url.clone())
            .send()
            .await
            .map_err(UiError::Http)?;
        if response.status() != StatusCode::OK {
            return Err(invalid(format!("download UI: HTTP {}", response.status())));
        }
        if response
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|length| length > MAX_ARCHIVE_SIZE)
        {
            return Err(invalid(format!(
                "UI archive exceeds {MAX_ARCHIVE_SIZE} bytes"
            )));
        }
        let mut size = 0_usize;
        let mut body = response.bytes_stream();
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(UiError::Http)?;
            size = size
                .checked_add(chunk.len())
                .filter(|size| *size <= MAX_ARCHIVE_SIZE)
                .ok_or_else(|| invalid(format!("UI archive exceeds {MAX_ARCHIVE_SIZE} bytes")))?;
            target.write_all(&chunk).await.map_err(UiError::Write)?;
        }
        target.flush().await.map_err(UiError::Write)?;
        drop(target);
        let source = if source.is_empty() {
            url.as_str()
        } else {
            source
        };
        self.install_file(temporary.path(), source_type, source, expected_digest)
    }

    pub fn remove(&self) -> Result<(), UiError> {
        match fs::remove_dir_all(self.current_dir()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(UiError::Write(error)),
        }
    }

    fn activate(&self, staging: &Path) -> Result<(), UiError> {
        fs::create_dir_all(&self.root).map_err(UiError::Write)?;
        let current = self.current_dir();
        let previous = self.root.join("current.previous");
        let _ = fs::remove_dir_all(&previous);
        if current.exists() {
            fs::rename(&current, &previous).map_err(UiError::Write)?;
        }
        if let Err(error) = fs::rename(staging, &current) {
            if previous.exists() {
                let _ = fs::rename(&previous, &current);
            }
            return Err(UiError::Write(error));
        }
        match fs::remove_dir_all(&previous) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(UiError::Write(error)),
        }
    }
}

fn extract(path: &Path, destination: &Path) -> Result<(), UiError> {
    let source = File::open(path).map_err(UiError::Read)?;
    let mut archive = zip::ZipArchive::new(source).map_err(UiError::Zip)?;
    if archive.len() > MAX_EXTRACTED_FILES {
        return Err(invalid(format!(
            "UI archive contains more than {MAX_EXTRACTED_FILES} entries"
        )));
    }
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(UiError::Zip)?;
        let entry_size = entry.size();
        expanded = expanded
            .checked_add(entry_size)
            .filter(|size| *size <= MAX_EXPANDED_SIZE)
            .ok_or_else(|| {
                invalid(format!(
                    "UI archive expands beyond {MAX_EXPANDED_SIZE} bytes"
                ))
            })?;
        let target = safe_target(destination, entry.name())?;
        let mode = entry
            .unix_mode()
            .unwrap_or(if entry.is_dir() { 0o040_700 } else { 0o100_600 });
        if mode & 0o170_000 == 0o120_000 {
            return Err(invalid(format!(
                "UI archive contains a symbolic link {:?}",
                entry.name()
            )));
        }
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(UiError::Write)?;
            continue;
        }
        if mode & 0o170_000 != 0o100_000 {
            return Err(invalid(format!(
                "UI archive contains an unsupported entry {:?}",
                entry.name()
            )));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(UiError::Write)?;
        }
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)
            .map_err(UiError::Write)?;
        let copied = std::io::copy(
            &mut entry.by_ref().take(entry_size.saturating_add(1)),
            &mut output,
        )
        .map_err(UiError::Write)?;
        if copied != entry_size {
            return Err(invalid(format!(
                "UI archive entry {:?} has an invalid size",
                entry.name()
            )));
        }
    }
    Ok(())
}

fn safe_target(root: &Path, name: &str) -> Result<PathBuf, UiError> {
    if name.contains('\\') {
        return Err(invalid(format!(
            "UI archive entry has an invalid path: {name:?}"
        )));
    }
    let mut target = root.to_path_buf();
    for component in Path::new(name).components() {
        match component {
            Component::Normal(value) => target.push(value),
            Component::CurDir => {}
            _ => {
                return Err(invalid(format!(
                    "UI archive entry escapes its root: {name:?}"
                )));
            }
        }
    }
    Ok(target)
}

fn read_manifest(root: &Path) -> Result<Manifest, UiError> {
    let data = fs::read(root.join(MANIFEST_NAME)).map_err(UiError::Read)?;
    let manifest = serde_json::from_slice(&data).map_err(UiError::Decode)?;
    validate(&manifest)?;
    if !root.join("index.html").is_file() {
        return Err(invalid("UI entry \"index.html\" is unavailable"));
    }
    Ok(manifest)
}

fn write_metadata(root: &Path, metadata: &Metadata) -> Result<(), UiError> {
    let mut data = serde_json::to_vec_pretty(metadata).map_err(UiError::Decode)?;
    data.push(b'\n');
    sempre_state::write_atomic(&root.join(METADATA_NAME), &data, 0o600).map_err(UiError::Write)
}

fn verify_digest(expected: &str, actual: &str) -> Result<(), UiError> {
    if expected.is_empty() {
        return Ok(());
    }
    let expected = expected.strip_prefix("sha256:").unwrap_or(expected);
    if expected.len() != 64
        || !expected.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !expected.eq_ignore_ascii_case(actual)
    {
        return Err(invalid(format!(
            "UI SHA-256 mismatch: expected {expected}, got {actual}"
        )));
    }
    Ok(())
}

fn valid_https_url(value: &str) -> Result<Url, UiError> {
    let url = Url::parse(value).map_err(|_| invalid("UI URL must be a valid HTTPS URL"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid("UI URL must be an HTTPS URL without credentials"));
    }
    Ok(url)
}

fn https_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            attempt.error("too many redirects")
        } else if attempt.url().scheme() != "https" {
            attempt.error("refuse non-HTTPS redirect")
        } else {
            attempt.follow()
        }
    })
}

fn invalid(message: impl Into<String>) -> UiError {
    UiError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive() -> Vec<u8> {
        let mut data = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut data);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file(MANIFEST_NAME, options).expect("manifest");
            zip.write_all(br#"{"schema":1,"name":"Sempre UI","version":"1","entry":"index.html","api":{"major":1}}"#)
                .expect("manifest data");
            zip.start_file("index.html", options).expect("entry");
            zip.write_all(b"<main>Sempre</main>").expect("entry data");
            zip.finish().expect("finish archive");
        }
        data.into_inner()
    }

    #[test]
    fn installs_activates_and_removes_a_verified_ui_archive() {
        let root = tempfile::tempdir().expect("temporary directory");
        let store = Store::new(root.path().join("ui"));
        let data = archive();
        let digest = format!("{:x}", Sha256::digest(&data));
        let metadata = store
            .install_bytes(&data, "local", "test.zip", &digest)
            .expect("install UI");
        assert_eq!(metadata.manifest.name, "Sempre UI");
        assert_eq!(store.current().expect("current UI"), metadata);
        store.remove().expect("remove UI");
        assert!(store.current().is_err());
    }

    #[test]
    fn rejects_digest_mismatch_and_path_escape() {
        let root = tempfile::tempdir().expect("temporary directory");
        let store = Store::new(root.path().join("ui"));
        assert!(
            store
                .install_bytes(&archive(), "local", "test.zip", &"0".repeat(64))
                .is_err()
        );

        let mut data = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut data);
            zip.start_file("../escape", zip::write::SimpleFileOptions::default())
                .expect("escape");
            zip.write_all(b"escape").expect("escape data");
            zip.finish().expect("finish archive");
        }
        assert!(
            store
                .install_bytes(&data.into_inner(), "local", "bad.zip", "")
                .is_err()
        );
    }
}
