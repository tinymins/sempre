use std::{
    fs,
    path::{Path, PathBuf},
};

use sempre_state::{Document, Layout, write_atomic};

use crate::{
    BundleError, Export, METADATA_NAME, Metadata, PORTABLE_MARKER, copy_file, copy_optional_file,
    copy_tree, write_document, write_json, write_web_config, zip_directory,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseTarget {
    pub os: String,
    pub arch: String,
}

impl ReleaseTarget {
    pub fn new(os: impl Into<String>, arch: impl Into<String>) -> Result<Self, BundleError> {
        let target = Self {
            os: os.into(),
            arch: arch.into(),
        };
        if !matches!(target.os.as_str(), "windows" | "linux" | "darwin")
            || !matches!(target.arch.as_str(), "amd64" | "arm64")
        {
            return Err(BundleError::InvalidMetadata(format!(
                "unsupported release target {}/{}",
                target.os, target.arch
            )));
        }
        Ok(target)
    }

    fn package_name(&self) -> String {
        format!("sempre-{}-{}", self.os, self.arch)
    }

    fn executable_name(&self) -> &'static str {
        if self.os == "windows" {
            "sempre.exe"
        } else {
            "sempre"
        }
    }
}

pub fn package_release(
    source: &Layout,
    document: &Document,
    executable: &Path,
    output: &Path,
    target: &ReleaseTarget,
) -> Result<Export, BundleError> {
    fs::create_dir_all(output).map_err(|source_error| BundleError::Io {
        operation: "create release output directory",
        path: output.to_path_buf(),
        source: source_error,
    })?;
    fs::create_dir_all(&source.runtime).map_err(|source_error| BundleError::Io {
        operation: "create release staging directory",
        path: source.runtime.clone(),
        source: source_error,
    })?;
    let staging = tempfile::Builder::new()
        .prefix("release-bundle-")
        .tempdir_in(&source.runtime)
        .map_err(|source_error| BundleError::Io {
            operation: "create release staging directory",
            path: source.runtime.clone(),
            source: source_error,
        })?;
    let package_name = target.package_name();
    let package = staging.path().join(&package_name);
    let layout = Layout::portable_at(&package.join(target.executable_name()));
    layout.ensure().map_err(BundleError::Layout)?;
    write_release_directory(source, &layout, document, executable, target)?;

    let download_name = format!("sempre-bundle-{}-{}.zip", target.os, target.arch);
    let archive = output.join(&download_name);
    if let Err(error) = zip_directory(&archive, &package, &package_name) {
        let _ = fs::remove_file(&archive);
        return Err(error);
    }
    Ok(Export {
        archive,
        download_name,
    })
}

fn write_release_directory(
    source: &Layout,
    target: &Layout,
    document: &Document,
    executable: &Path,
    release: &ReleaseTarget,
) -> Result<(), BundleError> {
    copy_file(executable, &target.service_executable, true)?;
    for (from, to) in [
        (&source.resources, &target.resources),
        (&source.tools, &target.tools),
        (&source.cores, &target.cores),
        (&source.configs, &target.configs),
        (&source.subscriptions, &target.subscriptions),
        (&source.gateway, &target.gateway),
        (&source.ui, &target.ui),
    ] {
        copy_tree(from, to)?;
    }
    copy_optional_file(&source.tunnels, &target.tunnels)?;
    write_document(&target.state, document)?;
    write_web_config(&source.web_config, &target.web_config)?;
    write_json(
        &target.root.join(METADATA_NAME),
        &Metadata {
            schema: 1,
            kind: "release",
        },
    )?;
    write_atomic(&target.root.join(PORTABLE_MARKER), b"", 0o600).map_err(|source_error| {
        BundleError::Io {
            operation: "write portable marker",
            path: target.root.join(PORTABLE_MARKER),
            source: source_error,
        }
    })?;
    write_installer(
        &target.root,
        target.service_executable.file_name().unwrap_or_default(),
        release,
    )
}

fn write_installer(
    directory: &Path,
    executable: &std::ffi::OsStr,
    target: &ReleaseTarget,
) -> Result<(), BundleError> {
    let executable = executable.to_string_lossy();
    let unix = format!(
        "#!/bin/sh\nset -eu\ncd -- \"$(dirname -- \"$0\")\"\n\"./{executable}\" install \"$@\"\n"
    );
    match target.os.as_str() {
        "windows" => write(
            directory.join("install.cmd"),
            format!(
                "@echo off\r\ncd /d \"%~dp0\"\r\n\"%~dp0{executable}\" install %*\r\nset EXITCODE=%ERRORLEVEL%\r\npause\r\nexit /b %EXITCODE%\r\n"
            )
            .as_bytes(),
        ),
        "darwin" => {
            write(directory.join("install.command"), unix.as_bytes())?;
            write(directory.join("install.sh"), unix.as_bytes())
        }
        _ => {
            write(directory.join("install.sh"), unix.as_bytes())?;
            write(
                directory.join("install.desktop"),
                b"[Desktop Entry]\nType=Application\nName=Install Sempre\nTerminal=true\nExec=sh -c 'cd \"$(dirname \"$1\")\" && sh install.sh' sh %k\n",
            )
        }
    }
}

fn write(path: PathBuf, content: &[u8]) -> Result<(), BundleError> {
    write_atomic(&path, content, 0o755).map_err(|source| BundleError::Io {
        operation: "write release installer",
        path,
        source,
    })
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Read as _};

    use sempre_state::Store;

    use super::*;

    #[test]
    fn release_archive_uses_target_names_and_install_entrypoints() {
        for (os, installers, absent) in [
            ("windows", &["install.cmd"][..], &["install.sh"][..]),
            (
                "linux",
                &["install.sh", "install.desktop"][..],
                &["install.cmd"][..],
            ),
            (
                "darwin",
                &["install.command", "install.sh"][..],
                &["install.cmd"][..],
            ),
        ] {
            let temporary = tempfile::tempdir().expect("temporary directory");
            let source = Layout::at(&temporary.path().join("source"));
            let document = Store::new(source.clone()).initialize().expect("state");
            fs::write(
                &source.web_config,
                b"{\"schema\":1,\"listen\":\"127.0.0.1:33211\"}",
            )
            .expect("web config");
            let executable = temporary.path().join("sempre-source");
            fs::write(&executable, b"binary").expect("binary");
            let target = ReleaseTarget::new(os, "amd64").expect("target");
            let result =
                package_release(&source, &document, &executable, temporary.path(), &target)
                    .expect("release bundle");
            assert_eq!(
                result.download_name,
                format!("sempre-bundle-{os}-amd64.zip")
            );
            let mut archive = zip::ZipArchive::new(File::open(result.archive).expect("archive"))
                .expect("release ZIP");
            let prefix = format!("sempre-{os}-amd64");
            let executable = if os == "windows" {
                "sempre.exe"
            } else {
                "sempre"
            };
            assert!(archive.by_name(&format!("{prefix}/{executable}")).is_ok());
            for installer in installers {
                let mut entry = archive
                    .by_name(&format!("{prefix}/{installer}"))
                    .expect("installer");
                let mut content = String::new();
                entry.read_to_string(&mut content).expect("installer text");
                if *installer == "install.desktop" {
                    assert!(content.contains("sh install.sh"));
                } else {
                    assert!(content.contains(" install "));
                }
                assert!(!content.contains("--yes"));
            }
            for name in absent {
                assert!(archive.by_name(&format!("{prefix}/{name}")).is_err());
            }
            let metadata: serde_json::Value = {
                let entry = archive
                    .by_name(&format!("{prefix}/{METADATA_NAME}"))
                    .expect("metadata");
                serde_json::from_reader(entry).expect("metadata JSON")
            };
            assert_eq!(metadata["kind"], "release");
        }
    }
}
