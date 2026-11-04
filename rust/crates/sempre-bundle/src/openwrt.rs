//! An `OpenWrt` package is the executable, UI, and one core in a tar.gz, because `BusyBox`
//! firmware has tar but not always unzip. `sempre install` picks the files up from beside
//! the executable, so the package needs no bundle metadata.
use std::{fs, path::Path, path::PathBuf};

use sempre_state::write_atomic;

use crate::{BundleError, ReleaseTarget, archive::tar_gz_directory, copy_file, copy_optional_file};

const INSTALLER: &[u8] =
    b"#!/bin/sh\nset -eu\ncd -- \"$(dirname -- \"$0\")\"\n./sempre install \"$@\"\n";

pub struct OpenwrtPackage<'a> {
    pub name: &'a str,
    pub core: &'a str,
    pub executable: &'a Path,
    pub ui_archive: &'a Path,
    pub core_directory: &'a Path,
    pub output: &'a Path,
    pub target: &'a ReleaseTarget,
}

pub fn package_openwrt(package: &OpenwrtPackage<'_>) -> Result<PathBuf, BundleError> {
    let io = |operation, path: &Path, source| BundleError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    };
    let staging =
        tempfile::tempdir().map_err(|error| io("create package staging", package.output, error))?;
    let root = staging.path().join(package.name);
    fs::create_dir_all(&root).map_err(|error| io("create package directory", &root, error))?;
    copy_file(package.executable, &root.join("sempre"), true)?;
    copy_file(package.ui_archive, &root.join("sempre-ui.zip"), false)?;
    copy_file(
        &package.core_directory.join(package.core),
        &root.join(package.core),
        true,
    )?;
    // Sempre has its own license, so the core's is named after the core.
    copy_optional_file(
        &package.core_directory.join("LICENSE"),
        &root.join(format!("{}.LICENSE", package.core)),
    )?;
    let installer = root.join("install.sh");
    write_atomic(&installer, INSTALLER, 0o755)
        .map_err(|error| io("write package installer", &installer, error))?;
    let archive = package.output.join(format!("{}.tar.gz", package.name));
    if let Err(error) = tar_gz_directory(&archive, &root, package.name, package.target) {
        let _ = fs::remove_file(&archive);
        return Err(error);
    }
    Ok(archive)
}

#[cfg(all(test, unix))]
mod tests {
    use flate2::read::GzDecoder;

    use super::*;

    #[test]
    fn package_holds_one_core_and_keeps_its_programs_executable() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let input = temporary.path();
        let core_directory = input.join("core");
        fs::create_dir_all(&core_directory).expect("core directory");
        fs::write(core_directory.join("sing-box"), b"core").expect("core");
        fs::write(core_directory.join("LICENSE"), b"license").expect("license");
        fs::write(input.join("executable"), b"sempre").expect("executable");
        fs::write(input.join("ui.zip"), b"ui").expect("ui");
        let name = "sempre-openwrt-sing-box-1.14.4-x86_64";
        let archive = package_openwrt(&OpenwrtPackage {
            name,
            core: "sing-box",
            executable: &input.join("executable"),
            ui_archive: &input.join("ui.zip"),
            core_directory: &core_directory,
            output: input,
            target: &ReleaseTarget::new("linux", "amd64").expect("target"),
        })
        .expect("package");
        assert_eq!(archive, input.join(format!("{name}.tar.gz")));

        let mut entries = tar::Archive::new(GzDecoder::new(fs::File::open(archive).expect("tar")))
            .entries()
            .expect("entries")
            .map(|entry| {
                let entry = entry.expect("entry");
                let path = entry.path().expect("path").to_string_lossy().into_owned();
                (path, entry.header().mode().expect("mode") & 0o111 != 0)
            })
            .collect::<Vec<_>>();
        entries.sort();
        let expected = [
            ("", true),
            ("/install.sh", true),
            ("/sempre", true),
            ("/sempre-ui.zip", false),
            ("/sing-box", true),
            ("/sing-box.LICENSE", false),
        ]
        .map(|(file, executable)| (format!("{name}{file}"), executable));
        // A tar directory entry keeps its trailing separator.
        entries[0].0 = entries[0].0.trim_end_matches('/').to_owned();
        assert_eq!(entries, expected);
    }
}
