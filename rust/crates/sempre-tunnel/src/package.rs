use std::{
    fs,
    path::{Path, PathBuf},
};

use sempre_artifact::{ArchiveFormat, Artifact, Downloader, ExtractOptions, extract, find};
use sempre_state::Layout;

use crate::{BinaryStatus, TunnelError};

pub const VERSION: &str = "10.5.5";

struct Package {
    name: String,
    url: String,
    digest: String,
    size: u64,
}

pub(crate) fn binary_status(layout: &Layout) -> BinaryStatus {
    BinaryStatus {
        version: VERSION.into(),
        installed: binary_path(layout).is_file(),
    }
}

pub(crate) fn binary_path(layout: &Layout) -> PathBuf {
    tool_directory(layout).join(executable_name(std::env::consts::OS))
}

pub(crate) async fn install(
    layout: &Layout,
    downloader: &Downloader,
) -> Result<(PathBuf, bool), TunnelError> {
    install_for(
        layout,
        downloader,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
    .await
}

pub async fn install_for(
    layout: &Layout,
    downloader: &Downloader,
    os: &str,
    arch: &str,
) -> Result<(PathBuf, bool), TunnelError> {
    let binary = tool_directory(layout).join(executable_name(os));
    if binary.is_file() {
        return Ok((binary, false));
    }
    let package = package_for(os, arch)?;
    let temporary = tempfile::Builder::new()
        .prefix("wstunnel-install-")
        .tempdir_in(&layout.runtime)
        .map_err(|error| TunnelError::io("create wstunnel install directory", error))?;
    let archive = temporary.path().join(&package.name);
    downloader
        .verified(
            &Artifact {
                name: package.name,
                url: package.url,
                digest: package.digest,
                size: package.size,
            },
            &archive,
        )
        .await?;
    let extracted = temporary.path().join("extract");
    extract(
        &archive,
        &extracted,
        &ExtractOptions {
            format: ArchiveFormat::TarGz,
            single_file_name: Some(executable_name(os)),
        },
    )?;
    let source = find(&extracted, &executable_name(os))?;
    activate(layout, &source, os)?;
    Ok((binary, true))
}

fn activate(layout: &Layout, source: &Path, os: &str) -> Result<(), TunnelError> {
    let final_directory = tool_directory(layout);
    let parent = final_directory
        .parent()
        .ok_or_else(|| TunnelError::invalid("wstunnel tool directory has no parent"))?;
    fs::create_dir_all(parent)
        .map_err(|error| TunnelError::io("create wstunnel tool directory", error))?;
    let staging = tempfile::Builder::new()
        .prefix(".wstunnel-")
        .tempdir_in(parent)
        .map_err(|error| TunnelError::io("create wstunnel staging directory", error))?;
    let staged = staging.path().join(executable_name(os));
    fs::copy(source, &staged).map_err(|error| TunnelError::io("stage wstunnel binary", error))?;
    make_executable(&staged)?;
    if final_directory.exists() {
        fs::remove_dir_all(&final_directory)
            .map_err(|error| TunnelError::io("remove incomplete wstunnel installation", error))?;
    }
    fs::rename(staging.path(), &final_directory)
        .map_err(|error| TunnelError::io("activate wstunnel", error))?;
    let _ = staging.keep();
    Ok(())
}

fn tool_directory(layout: &Layout) -> PathBuf {
    layout.tools.join("wstunnel").join(VERSION)
}

fn executable_name(os: &str) -> String {
    if os == "windows" {
        "wstunnel.exe"
    } else {
        "wstunnel"
    }
    .into()
}

fn package_for(os: &str, arch: &str) -> Result<Package, TunnelError> {
    let (target, digest, size) = match (os, arch) {
        ("windows", "x86_64" | "aarch64") => (
            "windows_amd64",
            "d77ab72a96247000d9a6da1f0789d7306eb33b5466deafa1348b75bafb03cbce",
            4_077_936,
        ),
        ("linux", "x86_64") => (
            "linux_amd64",
            "b20ffa02e945ec0c0d6b153ba69a290593f0957ed2892aee8f987f715ccd95d6",
            4_983_919,
        ),
        ("linux", "aarch64") => (
            "linux_arm64",
            "db85183da9732f26c110a08e3fffdfcfc4a44d544035d01eeefa708ed23874bb",
            4_601_463,
        ),
        ("macos", "x86_64") => (
            "darwin_amd64",
            "83515a275775d4f3730315ae86762234f0fc0ec646826c9aaa0106adde0f25b0",
            4_573_839,
        ),
        ("macos", "aarch64") => (
            "darwin_arm64",
            "c905eb5a54a31e0f4639d1676226a7790dcd9d2787364d3332613cdf0a67c36f",
            4_242_096,
        ),
        _ => {
            return Err(TunnelError::invalid(format!(
                "wstunnel {VERSION} is unavailable for {os}/{arch}"
            )));
        }
    };
    let name = format!("wstunnel_{VERSION}_{target}.tar.gz");
    Ok(Package {
        url: format!("https://github.com/erebe/wstunnel/releases/download/v{VERSION}/{name}"),
        name,
        digest: format!("sha256:{digest}"),
        size,
    })
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), TunnelError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|error| TunnelError::io("make wstunnel executable", error))
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn make_executable(_path: &Path) -> Result<(), TunnelError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_metadata_covers_supported_targets() {
        for (os, arch) in [
            ("windows", "x86_64"),
            ("windows", "aarch64"),
            ("linux", "x86_64"),
            ("linux", "aarch64"),
            ("macos", "x86_64"),
            ("macos", "aarch64"),
        ] {
            let package = package_for(os, arch).expect("package");
            assert!(package.digest.starts_with("sha256:"));
            assert!(package.size > 0);
        }
    }
}
