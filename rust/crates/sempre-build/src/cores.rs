use std::{fs, path::Path};

use chrono::{DateTime, Utc};
use sempre_artifact::{ArchiveFormat, Artifact, Downloader, ExtractOptions, GithubClient};
use sempre_core::{Package, STABLE, Target, built_in_registry};
use sempre_state::{Document, Installation, Layout, Selection};

use crate::BuildError;

const SING_BOX_V11: &str = "1.11.15";
const SING_BOX_V12: &str = "1.12.20";
const SING_BOX_V13: &str = "1.13.18";
const SING_BOX_V14: &str = "1.14.0-beta.13";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Request {
    core: &'static str,
    reference: &'static str,
    channel: Option<&'static str>,
}

#[derive(Clone, Debug)]
struct Resolved {
    core: String,
    channel: Option<String>,
    package: Package,
}

pub(crate) async fn install_bundled_cores(
    layout: &Layout,
    target: &Target,
    installed_at: DateTime<Utc>,
) -> Result<Document, BuildError> {
    let registry = built_in_registry();
    let releases = GithubClient::new("Sempre release builder")?;
    let downloader = Downloader::new("Sempre release builder")?;
    let mut resolved = Vec::new();
    for request in requests() {
        let adapter = registry.get(request.core)?;
        let package = releases
            .resolve(adapter.as_ref(), "", request.reference, target)
            .await?;
        install_core(layout, target, adapter.as_ref(), &package, &downloader).await?;
        resolved.push(Resolved {
            core: request.core.into(),
            channel: request.channel.map(str::to_owned),
            package,
        });
    }
    build_document(installed_at, &resolved)
}

fn requests() -> [Request; 7] {
    [
        Request {
            core: "sing-box",
            reference: SING_BOX_V11,
            channel: None,
        },
        Request {
            core: "sing-box",
            reference: SING_BOX_V12,
            channel: None,
        },
        Request {
            core: "sing-box",
            reference: SING_BOX_V13,
            channel: Some(STABLE),
        },
        Request {
            core: "sing-box",
            reference: SING_BOX_V14,
            channel: None,
        },
        Request {
            core: "mihomo",
            reference: STABLE,
            channel: Some(STABLE),
        },
        Request {
            core: "xray",
            reference: STABLE,
            channel: Some(STABLE),
        },
        Request {
            core: "v2ray",
            reference: STABLE,
            channel: Some(STABLE),
        },
    ]
}

async fn install_core(
    layout: &Layout,
    target: &Target,
    adapter: &dyn sempre_core::Adapter,
    package: &Package,
    downloader: &Downloader,
) -> Result<(), BuildError> {
    let temporary = tempfile::Builder::new()
        .prefix("release-core-")
        .tempdir_in(&layout.runtime)
        .map_err(|source| {
            BuildError::io("create core staging directory", &layout.runtime, source)
        })?;
    let archive = temporary.path().join(&package.name);
    downloader
        .verified(
            &Artifact {
                name: package.name.clone(),
                url: package.url.clone(),
                digest: package.digest.clone(),
                size: package.size,
            },
            &archive,
        )
        .await?;
    let executable_name = adapter.executable_name(target)?;
    let extracted = temporary.path().join("extract");
    sempre_artifact::extract(
        &archive,
        &extracted,
        &ExtractOptions {
            format: ArchiveFormat::try_from(package.format.as_str())?,
            single_file_name: Some(executable_name.clone()),
        },
    )?;
    let executable = sempre_artifact::find(&extracted, &executable_name)?;
    let source = executable
        .parent()
        .ok_or_else(|| BuildError::invalid("core executable has no parent directory"))?;
    let destination = layout.core_version_dir(adapter.id(), None, &package.version);
    copy_tree(source, &destination)?;
    let copied = destination.join(
        executable
            .file_name()
            .ok_or_else(|| BuildError::invalid("core executable has no file name"))?,
    );
    let final_path = destination.join(executable_name);
    if copied != final_path {
        fs::rename(&copied, &final_path)
            .map_err(|source| BuildError::io("rename core executable", &final_path, source))?;
    }
    make_executable(&final_path)?;
    Ok(())
}

fn build_document(
    installed_at: DateTime<Utc>,
    installations: &[Resolved],
) -> Result<Document, BuildError> {
    let mut document = Document {
        selected: Some(Selection {
            core: "sing-box".into(),
            repository: None,
            reference: STABLE.into(),
        }),
        ..Document::default()
    };
    for installation in installations {
        let source = &mut document.core_mut(&installation.core).default;
        if let Some(channel) = &installation.channel {
            source
                .channels
                .insert(channel.clone(), installation.package.version.clone());
        }
        source.installed.insert(
            installation.package.version.clone(),
            Installation {
                explicit: false,
                digest: installation.package.digest.clone(),
                source: installation.package.url.clone(),
                installed_at,
            },
        );
    }
    document
        .validate()
        .map_err(|error| BuildError::invalid(format!("invalid release state: {error}")))?;
    Ok(document)
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), BuildError> {
    fs::create_dir_all(target)
        .map_err(|error| BuildError::io("create core directory", target, error))?;
    for entry in fs::read_dir(source)
        .map_err(|error| BuildError::io("read core directory", source, error))?
    {
        let entry = entry.map_err(|error| BuildError::io("read core entry", source, error))?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let kind = entry
            .file_type()
            .map_err(|error| BuildError::io("inspect core entry", &from, error))?;
        if kind.is_dir() {
            copy_tree(&from, &to)?;
        } else if kind.is_file() {
            fs::copy(&from, &to).map_err(|error| BuildError::io("copy core file", &to, error))?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), BuildError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|error| BuildError::io("make core executable", path, error))
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<(), BuildError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use sempre_core::Registry;

    use super::*;

    fn package(version: &str) -> Package {
        Package {
            version: version.into(),
            name: format!("core-{version}.zip"),
            url: format!("https://example.invalid/{version}"),
            digest: format!("sha256:{}", "a".repeat(64)),
            size: 42,
            format: "zip".into(),
        }
    }

    #[test]
    fn release_state_selects_stable_sing_box_and_keeps_all_versions() {
        let installed_at = Utc::now();
        let resolved = requests()
            .iter()
            .enumerate()
            .map(|(index, request)| Resolved {
                core: request.core.into(),
                channel: request.channel.map(str::to_owned),
                package: package(&format!("1.0.{index}")),
            })
            .collect::<Vec<_>>();
        let document = build_document(installed_at, &resolved).expect("release state");
        assert_eq!(
            document.selected,
            Some(Selection {
                core: "sing-box".into(),
                repository: None,
                reference: STABLE.into()
            })
        );
        assert_eq!(document.cores["sing-box"].default.installed.len(), 4);
        for core in ["sing-box", "mihomo", "xray", "v2ray"] {
            assert!(document.cores[core].default.channels.contains_key(STABLE));
        }
    }

    #[test]
    fn bundled_versions_cover_sing_box_v11_through_v14() {
        let references = requests()
            .iter()
            .filter(|request| request.core == "sing-box")
            .map(|request| request.reference)
            .collect::<Vec<_>>();
        assert_eq!(
            references,
            [SING_BOX_V11, SING_BOX_V12, SING_BOX_V13, SING_BOX_V14]
        );
        let registry: Registry = built_in_registry();
        for request in requests() {
            registry.get(request.core).expect("known bundled core");
        }
    }
}
