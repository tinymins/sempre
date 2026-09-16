use std::{path::Path, sync::Arc};

use sempre_artifact::ArchiveFormat;
use tokio::process::Command;

use super::{VERSION, executable_name, extract_release, parse_version, release_target};
use crate::service_update_task::ServiceUpdateTasks;

pub(crate) fn uploaded_archive_format(name: &str) -> Result<ArchiveFormat, String> {
    let path = Path::new(name);
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        Ok(ArchiveFormat::Zip)
    } else if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"))
        && path
            .file_stem()
            .and_then(|stem| Path::new(stem).extension())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("tar"))
    {
        Ok(ArchiveFormat::TarGz)
    } else {
        Err("Sempre update packages must be .zip or .tar.gz archives".into())
    }
}

pub(crate) fn start_uploaded(
    tasks: Arc<ServiceUpdateTasks>,
    task_id: String,
    temporary: tempfile::TempDir,
    archive_format: ArchiveFormat,
    allow_prerelease: bool,
) {
    tokio::spawn(async move {
        if let Err(error) = run_uploaded_task(
            &tasks,
            &task_id,
            temporary,
            archive_format,
            allow_prerelease,
        )
        .await
        {
            tasks.fail(&task_id, &error);
        }
    });
}

async fn run_uploaded_task(
    tasks: &ServiceUpdateTasks,
    task_id: &str,
    temporary: tempfile::TempDir,
    archive_format: ArchiveFormat,
    allow_prerelease: bool,
) -> Result<(), String> {
    let target = release_target()?;
    let archive = temporary.path().join("upload");
    let extracted = temporary.path().join("bundle");
    extract_release(tasks, task_id, &archive, &extracted, archive_format)?;
    tasks.set_stage(task_id, "validating")?;
    let canonical_root = extracted.join(format!("sempre-{target}"));
    let root = if canonical_root.is_dir() {
        canonical_root
    } else {
        extracted
    };
    validate_uploaded_entrypoints(&root)?;
    sempre_bundle::validate_release(&root).map_err(|error| error.to_string())?;
    let executable = root.join(executable_name());
    let version = reported_version(&executable).await?;
    let parsed = parse_version(&version)?;
    if !allow_prerelease && !parsed.pre.is_empty() {
        return Err("uploaded Sempre update is a prerelease; enable preview updates first".into());
    }
    if parsed <= parse_version(VERSION)? {
        return Err(format!(
            "uploaded Sempre version {version} is not newer than {VERSION}"
        ));
    }
    tasks.set_target_version(task_id, &version)?;
    tasks.set_stage(task_id, "installing")?;
    crate::service_update_schedule::schedule(temporary, &executable, tasks.installer_log_path())?;
    Ok(())
}

async fn reported_version(executable: &Path) -> Result<String, String> {
    let output = Command::new(executable)
        .arg("version")
        .output()
        .await
        .map_err(|error| format!("inspect Sempre executable in uploaded update: {error}"))?;
    if !output.status.success() {
        return Err("Sempre executable in uploaded update did not report its version".into());
    }
    let actual = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    actual
        .strip_prefix("Sempre ")
        .map(str::to_owned)
        .ok_or_else(|| format!("Sempre executable in uploaded update reports {actual:?}"))
}

pub(super) fn validate_uploaded_entrypoints(root: &Path) -> Result<(), String> {
    let installer = installer_name();
    if !root.join(".sempre").is_dir() || !root.join(installer).is_file() {
        return Err(format!(
            "uploaded update package must contain .sempre and {installer} at its root"
        ));
    }
    Ok(())
}

pub(super) fn installer_name() -> &'static str {
    installer_name_for_os(std::env::consts::OS)
}

pub(super) fn installer_name_for_os(os: &str) -> &'static str {
    match os {
        "windows" => "install.cmd",
        "macos" => "install.command",
        _ => "install.sh",
    }
}
