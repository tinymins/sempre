use std::{
    fs,
    path::{Path, PathBuf},
};

use sempre_artifact::{ArchiveFormat, Artifact, Downloader, ExtractOptions};

use crate::{BuildError, BuildTarget, checksum};

const ARCHIVE: &str = "WinDivert-2.2.2-A.zip";
const DIGEST: &str = "63cb41763bb4b20f600b6de04e991a9c2be73279e317d4d82f237b150c5f3f15";
const FILES: &[&str] = &[
    "sempre-dns-capture.exe",
    "WinDivert.dll",
    "WinDivert64.sys",
    "LICENSE",
];

pub fn dns_capture_supported(target: &BuildTarget) -> bool {
    target.os == "windows" && target.arch == "amd64"
}

pub async fn prepare_dns_capture(directory: &Path) -> Result<PathBuf, BuildError> {
    fs::create_dir_all(directory)
        .map_err(|error| BuildError::io("create DNS capture build directory", directory, error))?;
    let archive = directory.join(ARCHIVE);
    if !archive.is_file() || checksum::sha256(&archive)? != DIGEST {
        if archive.exists() {
            fs::remove_file(&archive).map_err(|error| {
                BuildError::io("remove invalid WinDivert archive", &archive, error)
            })?;
        }
        Downloader::new("Sempre release builder")?
            .verified(
                &Artifact {
                    name: ARCHIVE.into(),
                    url: format!(
                        "https://github.com/basil00/WinDivert/releases/download/v2.2.2/{ARCHIVE}"
                    ),
                    digest: format!("sha256:{DIGEST}"),
                    size: 405_137,
                },
                &archive,
            )
            .await?;
    }
    let extracted = directory.join("extract");
    if extracted.exists() {
        fs::remove_dir_all(&extracted)
            .map_err(|error| BuildError::io("replace WinDivert extraction", &extracted, error))?;
    }
    sempre_artifact::extract(
        &archive,
        &extracted,
        &ExtractOptions {
            format: ArchiveFormat::Zip,
            single_file_name: None,
        },
    )?;
    Ok(extracted.join("WinDivert-2.2.2-A"))
}

pub fn assemble_dns_capture(executable: &Path, distribution: &Path) -> Result<(), BuildError> {
    let directory = executable
        .parent()
        .expect("DNS capture executable directory")
        .join("dns-capture");
    fs::create_dir_all(&directory).map_err(|error| {
        BuildError::io("create bundled DNS capture directory", &directory, error)
    })?;
    for (source, name) in [
        (executable.to_path_buf(), FILES[0]),
        (distribution.join("x64/WinDivert.dll"), FILES[1]),
        (distribution.join("x64/WinDivert64.sys"), FILES[2]),
        (distribution.join("LICENSE"), FILES[3]),
    ] {
        fs::copy(&source, directory.join(name))
            .map_err(|error| BuildError::io("copy DNS capture resource", &source, error))?;
    }
    checksum::write(
        &directory,
        &FILES.iter().map(|name| (*name).into()).collect::<Vec<_>>(),
    )
}

pub(crate) fn bundle_dns_capture(
    executable: &Path,
    resources: &Path,
    target: &BuildTarget,
) -> Result<(), BuildError> {
    if !dns_capture_supported(target) {
        return Ok(());
    }
    let source = executable
        .parent()
        .expect("release executable directory")
        .join("dns-capture");
    let destination = resources.join("dns-capture");
    fs::create_dir_all(&destination)
        .map_err(|error| BuildError::io("create DNS capture resources", &destination, error))?;
    for name in FILES.iter().copied().chain(["SHA256SUMS"]) {
        fs::copy(source.join(name), destination.join(name)).map_err(|error| {
            BuildError::io("bundle DNS capture resource", source.join(name), error)
        })?;
    }
    Ok(())
}
