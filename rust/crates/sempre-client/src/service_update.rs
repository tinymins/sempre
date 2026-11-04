use std::{error::Error, path::Path, sync::Arc};

use axum::{
    Json,
    response::{IntoResponse, Response},
};
use reqwest::StatusCode;
use sempre_artifact::{ArchiveFormat, Artifact, Downloader, ExtractOptions, Sha256Digest};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::process::Command;
use url::Url;

use crate::{
    VERSION,
    service_update_task::{ServiceUpdateTask, ServiceUpdateTasks},
};

const STABLE_MANIFEST_URL: &str = "https://sempre.run/api/releases/latest.json";
const PREVIEW_MANIFEST_URL: &str = "https://sempre.run/api/releases/preview.json";
const MAX_MANIFEST_SIZE: usize = 1 << 20;

#[path = "service_update_manifest.rs"]
mod manifest;
#[path = "service_update_openwrt.rs"]
mod openwrt;
#[path = "service_update_upload.rs"]
mod upload;
#[cfg(test)]
use upload::{installer_name, installer_name_for_os, validate_uploaded_entrypoints};
pub(crate) use upload::{start_uploaded, uploaded_archive_format};

use manifest::fetch_manifest;
#[cfg(test)]
use manifest::validate_manifest;
use openwrt::OpenwrtAsset;

#[derive(Clone, Debug, Deserialize)]
struct Manifest {
    schema: u32,
    version: String,
    published_at: String,
    notes: String,
    repository: String,
    #[serde(default)]
    releases: Vec<ManifestRelease>,
    assets: Vec<ManifestAsset>,
    #[serde(default)]
    openwrt_assets: Vec<OpenwrtAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct ManifestRelease {
    version: String,
    published_at: String,
    notes: String,
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
pub(crate) struct ReleaseNote {
    pub(crate) version: String,
    pub(crate) published_at: String,
    pub(crate) notes: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Status {
    pub(crate) current_version: String,
    pub(crate) latest_version: String,
    pub(crate) update_available: bool,
    pub(crate) published_at: String,
    pub(crate) release_notes: String,
    pub(crate) release_history: Vec<ReleaseNote>,
    pub(crate) repository: String,
}

/// Stopping the service from its own Web UI would leave no way to start it again.
pub(crate) fn service_managed_by_procd(manager: &sempre_manager::Manager) -> Option<Response> {
    refuse_on_openwrt(manager, "On OpenWrt, use /etc/init.d/sempre")
}

fn refuse_on_openwrt(manager: &sempre_manager::Manager, message: &str) -> Option<Response> {
    manager.store().layout().openwrt.then(|| {
        (
            StatusCode::CONFLICT,
            Json(json!({ "error": { "code": "SERVICE_UNAVAILABLE", "message": message } })),
        )
            .into_response()
    })
}

pub(crate) async fn check(
    manager: &sempre_manager::Manager,
    allow_prerelease: bool,
) -> Result<Status, String> {
    status(
        &fetch_manifest(manager, allow_prerelease).await?,
        allow_prerelease,
    )
}

pub(crate) fn start(
    tasks: Arc<ServiceUpdateTasks>,
    manager: Arc<sempre_manager::Manager>,
    allow_prerelease: bool,
) -> Result<ServiceUpdateTask, String> {
    let task = tasks.begin()?;
    let task_id = task.id.clone();
    tokio::spawn(async move {
        if let Err(error) = run_task(&tasks, &task_id, &manager, allow_prerelease).await {
            tasks.fail(&task_id, &error);
        }
    });
    Ok(task)
}

async fn run_task(
    tasks: &ServiceUpdateTasks,
    task_id: &str,
    manager: &sempre_manager::Manager,
    allow_prerelease: bool,
) -> Result<(), String> {
    let manifest = fetch_manifest(manager, allow_prerelease).await?;
    let status = status(&manifest, allow_prerelease)?;
    if !status.update_available {
        return Err("Sempre is already up to date".into());
    }
    if manager.store().layout().openwrt {
        return openwrt::run_task(tasks, task_id, manager, &manifest).await;
    }
    let target = release_target()?;
    let (asset, archive_format) = release_asset(&manifest.assets, &target)
        .ok_or_else(|| format!("release {} has no asset for {target}", manifest.version))?;
    let artifact = release_artifact(asset, &target)?;
    tasks.set_release(task_id, &asset.name, asset.size)?;
    let temporary = tempfile::Builder::new()
        .prefix("sempre-update-")
        .tempdir()
        .map_err(|error| format!("create update directory: {error}"))?;
    let archive = temporary.path().join(&asset.name);
    tasks.set_stage(task_id, "downloading")?;
    download_release(tasks, task_id, manager, &artifact, &archive).await?;
    tasks.set_stage(task_id, "verifying")?;
    let extracted = temporary.path().join("bundle");
    extract_release(tasks, task_id, &archive, &extracted, archive_format)?;
    tasks.set_stage(task_id, "validating")?;
    let root = extracted.join(format!("sempre-{target}"));
    sempre_bundle::validate_release(&root).map_err(|error| error.to_string())?;
    let executable = root.join(executable_name());
    let version = validate_version(&executable, &manifest.version).await?;
    tasks.ready(task_id, &version, temporary, executable)?;
    Ok(())
}

pub(crate) fn confirm(
    tasks: &ServiceUpdateTasks,
    task_id: &str,
) -> Result<ServiceUpdateTask, String> {
    let prepared = tasks.confirm(task_id)?;
    crate::service_update_schedule::schedule(
        prepared.temporary,
        &prepared.executable,
        tasks.installer_log_path(),
    )?;
    tasks
        .snapshot()
        .ok_or_else(|| "Sempre update task is no longer available".into())
}

fn extract_release(
    tasks: &ServiceUpdateTasks,
    task_id: &str,
    archive: &Path,
    extracted: &Path,
    archive_format: ArchiveFormat,
) -> Result<(), String> {
    tasks.set_stage(task_id, "extracting")?;
    sempre_artifact::extract(
        archive,
        extracted,
        &ExtractOptions {
            format: archive_format,
            single_file_name: None,
        },
    )
    .map_err(|error| error.to_string())
}

async fn download_release(
    tasks: &ServiceUpdateTasks,
    task_id: &str,
    manager: &sempre_manager::Manager,
    artifact: &Artifact,
    archive: &Path,
) -> Result<(), String> {
    let user_agent = format!("Sempre/{VERSION}");
    let lease = manager
        .resource_egress(&artifact.url)
        .await
        .map_err(|error| error.to_string())?;
    if !lease.is_system() {
        // A proxy rule decides this request, so it never falls back to another path.
        let downloader = Downloader::with_egress(&user_agent, &lease.egress)
            .map_err(|error| describe_error(&error))?;
        let progress = |downloaded, total| tasks.download_progress(task_id, downloaded, total);
        return downloader
            .verified_with_progress(artifact, archive, progress)
            .await
            .map_err(|error| describe_error(&error));
    }
    let proxy =
        crate::service_update_proxy::downloader(manager, &user_agent).map(|proxy| async move {
            let progress = |downloaded, total| tasks.download_progress(task_id, downloaded, total);
            proxy
                .verified_with_progress(artifact, archive, progress)
                .await
                .map_err(|error| describe_error(&error))
        });
    crate::service_update_proxy::proxy_first(
        "download Sempre update",
        proxy,
        |attempted| {
            if attempted {
                tasks.restart_download(task_id)?;
            }
            Ok(())
        },
        || async {
            let direct = Downloader::new(&user_agent).map_err(|error| describe_error(&error))?;
            let progress = |downloaded, total| tasks.download_progress(task_id, downloaded, total);
            direct
                .verified_with_progress(artifact, archive, progress)
                .await
                .map_err(|error| describe_error(&error))
        },
    )
    .await
}

fn describe_error(error: &dyn Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        let detail = error.to_string();
        if !message.ends_with(&detail) {
            message.push_str(": ");
            message.push_str(&detail);
        }
        source = error.source();
    }
    message
}

fn status(manifest: &Manifest, allow_prerelease: bool) -> Result<Status, String> {
    let current = parse_version(VERSION)?;
    let latest = parse_version(&manifest.version)?;
    let mut history = Vec::new();
    for release in &manifest.releases {
        let version = parse_version(&release.version)?;
        if version > current && version <= latest && (allow_prerelease || version.pre.is_empty()) {
            history.push((
                version,
                ReleaseNote {
                    version: release.version.clone(),
                    published_at: release.published_at.clone(),
                    notes: release.notes.clone(),
                },
            ));
        }
    }
    if latest > current && !history.iter().any(|(version, _)| version == &latest) {
        history.push((
            latest.clone(),
            ReleaseNote {
                version: manifest.version.clone(),
                published_at: manifest.published_at.clone(),
                notes: manifest.notes.clone(),
            },
        ));
    }
    history.sort_by(|(left, _), (right, _)| right.cmp(left));
    let release_history = history
        .into_iter()
        .map(|(_, release)| release)
        .collect::<Vec<_>>();
    let release_notes = match release_history.as_slice() {
        [] => manifest.notes.clone(),
        [release] => release.notes.clone(),
        releases => releases
            .iter()
            .map(|release| format!("## v{}\n\n{}", release.version, release.notes.trim()))
            .collect::<Vec<_>>()
            .join("\n\n"),
    };
    Ok(Status {
        current_version: VERSION.strip_prefix('v').unwrap_or(VERSION).into(),
        latest_version: manifest.version.clone(),
        update_available: latest > current,
        published_at: manifest.published_at.clone(),
        release_notes,
        release_history,
        repository: manifest.repository.clone(),
    })
}

fn parse_version(value: &str) -> Result<Version, String> {
    Version::parse(value.strip_prefix('v').unwrap_or(value))
        .map_err(|error| format!("invalid Sempre version {value:?}: {error}"))
}

fn release_artifact(asset: &ManifestAsset, target: &str) -> Result<Artifact, String> {
    let zip = format!("sempre-bundle-{target}.zip");
    let tar_gz = format!("sempre-bundle-{target}.tar.gz");
    if asset.name != zip && asset.name != tar_gz {
        return Err(format!("update asset name must be {zip} or {tar_gz}"));
    }
    let url = Url::parse(&asset.url)
        .map_err(|_| format!("update asset {} has an invalid URL", asset.name))?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(format!("update asset {} has an invalid URL", asset.name));
    }
    let digest = format!("sha256:{}", asset.sha256)
        .parse::<Sha256Digest>()
        .map_err(|error| format!("update asset {}: {error}", asset.name))?;
    Ok(Artifact {
        name: asset.name.clone(),
        url: asset.url.clone(),
        digest: digest.to_string(),
        size: asset.size,
    })
}

fn release_asset<'a>(
    assets: &'a [ManifestAsset],
    target: &str,
) -> Option<(&'a ManifestAsset, ArchiveFormat)> {
    if !target.starts_with("windows-") {
        let tar_gz = format!("sempre-bundle-{target}.tar.gz");
        if let Some(asset) = assets
            .iter()
            .find(|asset| asset.target == target && asset.name == tar_gz)
        {
            return Some((asset, ArchiveFormat::TarGz));
        }
    }
    let zip = format!("sempre-bundle-{target}.zip");
    assets
        .iter()
        .find(|asset| asset.target == target && asset.name == zip)
        .map(|asset| (asset, ArchiveFormat::Zip))
}

async fn validate_version(executable: &Path, expected: &str) -> Result<String, String> {
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
    Ok(actual_version.to_owned())
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

#[cfg(test)]
#[path = "service_update_tests.rs"]
mod tests;
