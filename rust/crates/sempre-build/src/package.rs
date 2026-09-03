use std::{fs, path::PathBuf};

use chrono::{DateTime, Utc};
use sempre_artifact::Downloader;
use sempre_bundle::ReleaseTarget;
use sempre_state::{Layout, Store};

use crate::{BuildError, BuildTarget, checksum, cores};

#[derive(Clone, Debug)]
pub struct BuildInput {
    pub executable: PathBuf,
    pub ui_archive: PathBuf,
    pub output: PathBuf,
    pub version: String,
    pub installed_at: DateTime<Utc>,
    pub target: BuildTarget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildOutput {
    pub binary: PathBuf,
    pub bundle: PathBuf,
    pub ui_archive: PathBuf,
    pub checksums: PathBuf,
}

pub async fn package(input: &BuildInput) -> Result<BuildOutput, BuildError> {
    validate(input)?;
    fs::create_dir_all(&input.output)
        .map_err(|error| BuildError::io("create release output", &input.output, error))?;
    let staging = tempfile::tempdir().map_err(|error| {
        BuildError::io("create release staging directory", &input.output, error)
    })?;
    let source = Layout::at(&staging.path().join("release-source"));
    Store::new(source.clone()).initialize()?;
    sempre_control::WebConfigStore::new(&source.web_config).initialize()?;
    sempre_subscription::SubscriptionStore::new(source.clone()).initialize()?;
    sempre_tunnel::initialize(&source)?;

    let ui_digest = checksum::sha256(&input.ui_archive)?;
    sempre_ui::Store::new(&source.ui).install_file(
        &input.ui_archive,
        "bundled",
        "resources/sempre-ui.zip",
        &ui_digest,
    )?;
    fs::create_dir_all(&source.resources)
        .map_err(|error| BuildError::io("create release resources", &source.resources, error))?;
    let resource_ui = source.resources.join("sempre-ui.zip");
    fs::copy(&input.ui_archive, &resource_ui)
        .map_err(|error| BuildError::io("copy bundled UI", &resource_ui, error))?;
    let bundled_rules = sempre_subscription::Fetcher::new(
        sempre_subscription::SubscriptionStore::new(source.clone()),
    )?
    .bundle_system_rule_providers()
    .await?;
    checksum::write(
        &source.resources,
        &["sempre-ui.zip".into(), file_name(&bundled_rules)?],
    )?;
    crate::dns_capture::bundle_dns_capture(&input.executable, &source.resources, &input.target)?;

    let downloader = Downloader::new("Sempre release builder")?;
    let (tunnel_os, tunnel_arch) = input.target.tunnel_target();
    sempre_tunnel::install_for(&source, &downloader, tunnel_os, tunnel_arch).await?;
    let document =
        cores::install_bundled_cores(&source, &input.target.core_target(), input.installed_at)
            .await?;

    let binary = input.output.join(input.target.binary_name());
    fs::copy(&input.executable, &binary)
        .map_err(|error| BuildError::io("copy release binary", &binary, error))?;
    make_executable(&binary)?;
    let release_target = ReleaseTarget::new(&input.target.os, &input.target.arch)?;
    let bundle = sempre_bundle::package_release(
        &source,
        &document,
        &input.executable,
        &input.output,
        &release_target,
    )?
    .archive;
    let ui_archive = input.output.join("sempre-ui.zip");
    if input.ui_archive != ui_archive {
        fs::copy(&input.ui_archive, &ui_archive)
            .map_err(|error| BuildError::io("copy UI release archive", &ui_archive, error))?;
    }
    let names = [
        file_name(&binary)?,
        file_name(&bundle)?,
        file_name(&ui_archive)?,
    ];
    checksum::write(&input.output, &names)?;
    Ok(BuildOutput {
        binary,
        bundle,
        ui_archive,
        checksums: input.output.join("SHA256SUMS"),
    })
}

fn validate(input: &BuildInput) -> Result<(), BuildError> {
    if input.version.trim().is_empty() {
        return Err(BuildError::invalid("release version cannot be empty"));
    }
    if !input.executable.is_file() {
        return Err(BuildError::invalid(format!(
            "release executable is unavailable: {}",
            input.executable.display()
        )));
    }
    if !input.ui_archive.is_file() {
        return Err(BuildError::invalid(format!(
            "UI archive is unavailable: {}",
            input.ui_archive.display()
        )));
    }
    Ok(())
}

fn file_name(path: &std::path::Path) -> Result<String, BuildError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| {
            BuildError::invalid(format!("release path has no file name: {}", path.display()))
        })
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) -> Result<(), BuildError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|error| BuildError::io("make release binary executable", path, error))
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn make_executable(_: &std::path::Path) -> Result<(), BuildError> {
    Ok(())
}
