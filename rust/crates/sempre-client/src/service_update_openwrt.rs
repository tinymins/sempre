//! `OpenWrt` releases publish the program and UI as plain files, separately from the
//! packages carrying a core, so an update can leave the running core alone.
use std::{fs, path::Path};

use sempre_artifact::{Artifact, Sha256Digest};
use sempre_manager::Manager;
use serde::Deserialize;
use url::Url;

use super::{Manifest, ServiceUpdateTasks, download_release, executable_name, validate_version};

const UI_ARCHIVE: &str = "sempre-ui.zip";
const CORE: &str = "sing-box";

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct OpenwrtAsset {
    name: String,
    url: String,
    sha256: String,
    size: u64,
}

impl OpenwrtAsset {
    fn artifact(&self) -> Result<Artifact, String> {
        let url = Url::parse(&self.url)
            .map_err(|_| format!("update asset {} has an invalid URL", self.name))?;
        if url.scheme() != "https" || url.host_str().is_none() {
            return Err(format!("update asset {} has an invalid URL", self.name));
        }
        let digest = format!("sha256:{}", self.sha256)
            .parse::<Sha256Digest>()
            .map_err(|error| format!("update asset {}: {error}", self.name))?;
        Ok(Artifact {
            name: self.name.clone(),
            url: self.url.clone(),
            digest: digest.to_string(),
            size: self.size,
        })
    }
}

/// Replaces the program and UI, and leaves the core alone.
pub(super) async fn run_task(
    tasks: &ServiceUpdateTasks,
    task_id: &str,
    manager: &Manager,
    manifest: &Manifest,
) -> Result<(), String> {
    let (binary, ui) = assets(&manifest.openwrt_assets, &manifest.version)?;
    tasks.set_release(task_id, &binary.name, binary.size + ui.size)?;
    let temporary = tempfile::Builder::new()
        .prefix("sempre-update-")
        .tempdir()
        .map_err(|error| format!("create update directory: {error}"))?;
    tasks.set_stage(task_id, "downloading")?;
    let executable = temporary.path().join(executable_name());
    download_release(tasks, task_id, manager, &binary.artifact()?, &executable).await?;
    make_executable(&executable)?;
    let archive = temporary.path().join(UI_ARCHIVE);
    download_release(tasks, task_id, manager, &ui.artifact()?, &archive).await?;
    tasks.set_stage(task_id, "validating")?;
    let version = validate_version(&executable, &manifest.version).await?;
    tasks.ready(task_id, &version, temporary, executable)?;
    Ok(())
}

fn binary_name(arch: &str) -> Result<String, String> {
    match arch {
        "x86_64" => Ok("sempre-linux-musl-amd64".into()),
        "aarch64" => Ok("sempre-linux-musl-arm64".into()),
        value => Err(format!("unsupported update architecture {value}")),
    }
}

fn assets<'a>(
    assets: &'a [OpenwrtAsset],
    version: &str,
) -> Result<(&'a OpenwrtAsset, &'a OpenwrtAsset), String> {
    assets_for(assets, version, std::env::consts::ARCH)
}

fn assets_for<'a>(
    assets: &'a [OpenwrtAsset],
    version: &str,
    arch: &str,
) -> Result<(&'a OpenwrtAsset, &'a OpenwrtAsset), String> {
    let name = binary_name(arch)?;
    let find = |wanted: &str| {
        assets
            .iter()
            .find(|asset| asset.name == wanted)
            .ok_or_else(|| format!("release {version} has no {wanted}"))
    };
    Ok((find(&name)?, find(UI_ARCHIVE)?))
}

/// A package also carries a core, which an update must not install.
pub(crate) fn prepare_uploaded_package(root: &Path) -> Result<(), String> {
    for required in ["sempre", UI_ARCHIVE] {
        if !root.join(required).is_file() {
            return Err(format!(
                "uploaded update package must contain {required} at its root"
            ));
        }
    }
    match fs::remove_file(root.join(CORE)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("remove packaged core: {error}"))
        }
        _ => Ok(()),
    }
}

/// A package extracts to one directory named after it, or directly when repacked by hand.
pub(crate) fn package_root(extracted: &Path) -> Result<std::path::PathBuf, String> {
    if extracted.join("sempre").is_file() {
        return Ok(extracted.to_path_buf());
    }
    let mut directories = fs::read_dir(extracted)
        .map_err(|error| format!("read uploaded update: {error}"))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir());
    let first = directories.next();
    match (first, directories.next()) {
        (Some(root), None) => Ok(root),
        _ => Err("uploaded update package must contain sempre at its root".into()),
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("make update executable: {error}"))
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> OpenwrtAsset {
        OpenwrtAsset {
            name: name.into(),
            url: format!("https://example.test/{name}"),
            sha256: "a".repeat(64),
            size: 1,
        }
    }

    #[test]
    fn selects_the_program_for_this_architecture_with_the_ui() {
        let published = [
            asset("sempre-linux-musl-amd64"),
            asset("sempre-linux-musl-arm64"),
            asset(UI_ARCHIVE),
        ];
        let (binary, ui) = assets_for(&published, "2.0.0", "aarch64").expect("assets");
        assert_eq!(binary.name, "sempre-linux-musl-arm64");
        assert_eq!(ui.name, UI_ARCHIVE);
        assert!(binary.artifact().is_ok());
        assert!(assets_for(&published[..2], "2.0.0", "x86_64").is_err());
        assert!(assets_for(&published, "2.0.0", "mips").is_err());
    }

    #[test]
    fn an_uploaded_package_never_installs_its_core() {
        let root = tempfile::tempdir().expect("root");
        assert!(prepare_uploaded_package(root.path()).is_err());
        for name in ["sempre", UI_ARCHIVE, CORE] {
            fs::write(root.path().join(name), b"x").expect("file");
        }
        prepare_uploaded_package(root.path()).expect("prepare");
        assert!(!root.path().join(CORE).exists());
        assert!(root.path().join("sempre").exists());
    }

    #[test]
    fn finds_the_package_root_either_way() {
        let root = tempfile::tempdir().expect("root");
        let nested = root.path().join("sempre-openwrt-sing-box-1.13.21-x86_64");
        fs::create_dir_all(&nested).expect("directory");
        fs::write(nested.join("sempre"), b"x").expect("file");
        assert_eq!(package_root(root.path()).expect("nested"), nested);
        assert_eq!(package_root(&nested).expect("flat"), nested);
    }
}
