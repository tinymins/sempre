use std::{path::Path, time::Duration};

use reqwest::{Client, StatusCode};
use sempre_artifact::{ArchiveFormat, Artifact, Downloader, ExtractOptions};
use semver::Version;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use tokio::process::Command;
use url::Url;

use crate::VERSION;

const MANIFEST_URL: &str = "https://sempre.run/api/releases/latest.json";
const MAX_MANIFEST_SIZE: usize = 1 << 20;

#[derive(Clone, Debug, Deserialize)]
struct Manifest {
    schema: u32,
    version: String,
    published_at: String,
    notes: String,
    repository: String,
    assets: Vec<ManifestAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct ManifestAsset {
    target: String,
    name: String,
    url: String,
    sha256: String,
    size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Status {
    pub(crate) current_version: String,
    pub(crate) latest_version: String,
    pub(crate) update_available: bool,
    pub(crate) published_at: String,
    pub(crate) release_notes: String,
    pub(crate) repository: String,
}

pub(crate) async fn check() -> Result<Status, String> {
    status(&fetch_manifest().await?)
}

pub(crate) async fn prepare_and_schedule() -> Result<Status, String> {
    let manifest = fetch_manifest().await?;
    let status = status(&manifest)?;
    if !status.update_available {
        return Err("Sempre is already up to date".into());
    }
    let target = release_target()?;
    let asset = manifest
        .assets
        .iter()
        .find(|asset| asset.target == target)
        .ok_or_else(|| format!("release {} has no asset for {target}", manifest.version))?;
    validate_asset(asset, &target)?;
    let temporary = tempfile::Builder::new()
        .prefix("sempre-update-")
        .tempdir()
        .map_err(|error| format!("create update directory: {error}"))?;
    let archive = temporary.path().join(&asset.name);
    Downloader::new(&format!("Sempre/{VERSION}"))
        .map_err(|error| error.to_string())?
        .verified(
            &Artifact {
                name: asset.name.clone(),
                url: asset.url.clone(),
                digest: asset.sha256.clone(),
                size: asset.size,
            },
            &archive,
        )
        .await
        .map_err(|error| error.to_string())?;
    let extracted = temporary.path().join("bundle");
    sempre_artifact::extract(
        &archive,
        &extracted,
        &ExtractOptions {
            format: ArchiveFormat::Zip,
            single_file_name: None,
        },
    )
    .map_err(|error| error.to_string())?;
    let root = extracted.join(format!("sempre-{target}"));
    sempre_bundle::validate_release(&root).map_err(|error| error.to_string())?;
    let executable = root.join(executable_name());
    validate_version(&executable, &manifest.version).await?;
    schedule(temporary, &executable)?;
    Ok(status)
}

async fn fetch_manifest() -> Result<Manifest, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("too many redirects")
            } else if attempt.url().scheme() != "https" {
                attempt.error("refuse non-HTTPS redirect")
            } else {
                attempt.follow()
            }
        }))
        .user_agent(format!("Sempre/{VERSION}"))
        .build()
        .map_err(|error| format!("build update client: {error}"))?;
    let response = client
        .get(MANIFEST_URL)
        .send()
        .await
        .map_err(|error| format!("query Sempre update service: {error}"))?;
    if response.status() != StatusCode::OK {
        return Err(format!(
            "query Sempre update service: HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MANIFEST_SIZE as u64)
    {
        return Err("Sempre update manifest exceeds 1 MiB".into());
    }
    let body = response
        .bytes()
        .await
        .map_err(|error| format!("read Sempre update manifest: {error}"))?;
    if body.len() > MAX_MANIFEST_SIZE {
        return Err("Sempre update manifest exceeds 1 MiB".into());
    }
    let manifest: Manifest = serde_json::from_slice(&body)
        .map_err(|error| format!("decode Sempre update manifest: {error}"))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.schema != 1 {
        return Err(format!(
            "unsupported Sempre update manifest schema {}",
            manifest.schema
        ));
    }
    parse_version(&manifest.version)?;
    let repository = Url::parse(&manifest.repository)
        .map_err(|_| "update manifest has an invalid repository URL".to_string())?;
    if repository.scheme() != "https" || repository.host_str().is_none() {
        return Err("update manifest has an invalid repository URL".into());
    }
    if manifest.published_at.len() > 128 || manifest.notes.len() > 256 * 1024 {
        return Err("update manifest metadata is too large".into());
    }
    Ok(())
}

fn status(manifest: &Manifest) -> Result<Status, String> {
    let current = parse_version(VERSION)?;
    let latest = parse_version(&manifest.version)?;
    Ok(Status {
        current_version: VERSION.strip_prefix('v').unwrap_or(VERSION).into(),
        latest_version: manifest.version.clone(),
        update_available: latest > current,
        published_at: manifest.published_at.clone(),
        release_notes: manifest.notes.clone(),
        repository: manifest.repository.clone(),
    })
}

fn parse_version(value: &str) -> Result<Version, String> {
    Version::parse(value.strip_prefix('v').unwrap_or(value))
        .map_err(|error| format!("invalid Sempre version {value:?}: {error}"))
}

fn validate_asset(asset: &ManifestAsset, target: &str) -> Result<(), String> {
    let expected = format!("sempre-bundle-{target}.zip");
    if asset.name != expected {
        return Err(format!("update asset name must be {expected}"));
    }
    let url = Url::parse(&asset.url)
        .map_err(|_| format!("update asset {} has an invalid URL", asset.name))?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(format!("update asset {} has an invalid URL", asset.name));
    }
    Ok(())
}

async fn validate_version(executable: &Path, expected: &str) -> Result<(), String> {
    let output = Command::new(executable)
        .arg("version")
        .output()
        .await
        .map_err(|error| format!("inspect downloaded Sempre executable: {error}"))?;
    if !output.status.success() {
        return Err("downloaded Sempre executable did not report its version".into());
    }
    let actual = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let actual_version = actual
        .strip_prefix("Sempre ")
        .ok_or_else(|| format!("downloaded Sempre executable reports {actual:?}"))?;
    if parse_version(actual_version)? != parse_version(expected)? {
        return Err(format!(
            "downloaded Sempre executable reports {actual:?}, expected {expected}"
        ));
    }
    Ok(())
}

fn release_target() -> Result<String, String> {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        "linux" => "linux",
        "windows" => "windows",
        value => return Err(format!("unsupported update operating system {value}")),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        value => return Err(format!("unsupported update architecture {value}")),
    };
    Ok(format!("{os}-{arch}"))
}

fn executable_name() -> &'static str {
    if cfg!(windows) {
        "sempre.exe"
    } else {
        "sempre"
    }
}

fn schedule(temporary: TempDir, executable: &Path) -> Result<(), String> {
    let root = temporary.keep();
    let result = platform_schedule(executable, &root);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&root);
    }
    result
}

#[cfg(target_os = "linux")]
fn platform_schedule(executable: &Path, root: &Path) -> Result<(), String> {
    let unit = format!("sempre-update-{}", uuid::Uuid::new_v4());
    let script = "sleep 1; \"$1\" --portable install --yes; code=$?; rm -rf -- \"$2\"; exit $code";
    let status = std::process::Command::new("systemd-run")
        .args(["--quiet", "--collect", "--no-block", "--unit", &unit])
        .arg("/bin/sh")
        .args(["-c", script, "sempre-update"])
        .arg(executable)
        .arg(root)
        .status()
        .map_err(|error| format!("schedule systemd update: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("schedule systemd update: {status}"))
    }
}

#[cfg(target_os = "macos")]
fn platform_schedule(executable: &Path, root: &Path) -> Result<(), String> {
    let label = format!("io.sempre.update.{}", uuid::Uuid::new_v4());
    let script = "sleep 1; \"$1\" --portable install --yes; code=$?; rm -rf -- \"$2\"; exit $code";
    let status = std::process::Command::new("launchctl")
        .args([
            "submit",
            "-l",
            &label,
            "--",
            "/bin/sh",
            "-c",
            script,
            "sempre-update",
        ])
        .arg(executable)
        .arg(root)
        .status()
        .map_err(|error| format!("schedule launchd update: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("schedule launchd update: {status}"))
    }
}

#[cfg(target_os = "windows")]
fn platform_schedule(executable: &Path, root: &Path) -> Result<(), String> {
    let executable = executable
        .to_str()
        .ok_or_else(|| "update executable path is not Unicode".to_string())?;
    let root = root
        .to_str()
        .ok_or_else(|| "update directory path is not Unicode".to_string())?;
    let script = format!(
        "$ErrorActionPreference='Stop'; Start-Sleep -Seconds 1; $p=Start-Process -FilePath {} -ArgumentList '--portable install --yes' -PassThru -Wait; $code=$p.ExitCode; Remove-Item -LiteralPath {} -Recurse -Force -ErrorAction SilentlyContinue; exit $code",
        powershell_literal(executable),
        powershell_literal(root),
    );
    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("schedule Windows update: {error}"))
}

#[cfg(target_os = "windows")]
fn powershell_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_schedule(_: &Path, _: &Path) -> Result<(), String> {
    Err("Sempre updates are unavailable on this operating system".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: &str) -> Manifest {
        Manifest {
            schema: 1,
            version: version.into(),
            published_at: "2026-09-07T09:15:18Z".into(),
            notes: "Fixed update handling.".into(),
            repository: "https://example.com/sempre".into(),
            assets: Vec::new(),
        }
    }

    #[test]
    fn update_status_uses_semantic_version_ordering() {
        let newer = status(&manifest("999.0.0")).expect("newer status");
        assert!(newer.update_available);
        let older = status(&manifest("0.1.0")).expect("older status");
        assert!(!older.update_available);
    }

    #[test]
    fn manifest_rejects_untrusted_repository_and_asset_urls() {
        let mut value = manifest("2.0.8");
        value.repository = "http://example.com/sempre".into();
        assert!(validate_manifest(&value).is_err());
        let asset = ManifestAsset {
            target: "linux-amd64".into(),
            name: "sempre-bundle-linux-amd64.zip".into(),
            url: "http://example.com/sempre.zip".into(),
            sha256: "a".repeat(64),
            size: 1,
        };
        assert!(validate_asset(&asset, "linux-amd64").is_err());
    }
}
