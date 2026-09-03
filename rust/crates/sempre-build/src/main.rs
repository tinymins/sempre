use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use sempre_build::{BuildError, BuildInput, BuildTarget};

#[derive(Debug, Parser)]
#[command(
    name = "sempre-build",
    about = "Build one native Sempre release target"
)]
struct Arguments {
    /// Replace the workspace release version.
    #[arg(long, global = true)]
    version: Option<String>,
    /// Write the target artifacts to this directory.
    #[arg(long, default_value = "dist", global = true)]
    output: PathBuf,
    #[command(subcommand)]
    task: Option<Task>,
}

#[derive(Clone, Copy, Debug, Subcommand)]
enum Task {
    /// Download and verify the Windows x64 DNS capture SDK, then print its path.
    DnsCaptureSdk,
    /// Run Rust and frontend quality gates without packaging.
    Verify,
    /// Build and package only the current native release target.
    Package,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run(Arguments::parse()).await {
        eprintln!("build failed: {error}");
        std::process::exit(1);
    }
}

async fn run(arguments: Arguments) -> Result<(), BuildError> {
    let root = repository_root()?;
    let rust = root.join("rust");
    if matches!(arguments.task, Some(Task::DnsCaptureSdk)) {
        let sdk = sempre_build::prepare_dns_capture(&rust.join("target/windivert")).await?;
        println!("{}", sdk.join("x64").display());
        return Ok(());
    }
    let output = absolute_output(&root, &arguments.output)?;
    if !matches!(arguments.task, Some(Task::Package)) {
        verify(&root, &rust).await?;
    }
    if matches!(arguments.task, Some(Task::Verify)) {
        return Ok(());
    }
    build_release(&root, &rust, &output, arguments.version).await
}

async fn verify(root: &Path, rust: &Path) -> Result<(), BuildError> {
    let mut environment = Vec::new();
    let sdk;
    let search_path;
    if sempre_build::dns_capture_supported(&BuildTarget::current()?) {
        let directory = sempre_build::prepare_dns_capture(&rust.join("target/windivert"))
            .await?
            .join("x64");
        sdk = directory.to_string_lossy().into_owned();
        search_path = format!("{sdk};{}", std::env::var("PATH").unwrap_or_default());
        environment.extend([
            ("WINDIVERT_PATH", sdk.as_str()),
            ("PATH", search_path.as_str()),
        ]);
    }
    run_command(rust, "cargo", ["fmt", "--all", "--", "--check"], &[])?;
    run_command(rust, "cargo", ["test", "--workspace"], &environment)?;
    run_command(
        rust,
        "cargo",
        [
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        &environment,
    )?;
    for script in ["lint", "tsc"] {
        run_command(root, "bun", ["run", script], &[])?;
    }
    run_command(&root.join("ui"), "bun", ["run", "test"], &[])?;
    run_command(&root.join("site"), "bun", ["run", "test"], &[])?;
    Ok(())
}

async fn build_release(
    root: &Path,
    rust: &Path,
    output: &Path,
    version: Option<String>,
) -> Result<(), BuildError> {
    run_command(root, "bun", ["run", "build:ui"], &[])?;

    let version = release_version(version);
    let commit = git(root, ["rev-parse", "--short=12", "HEAD"]);
    let date = git(root, ["show", "-s", "--format=%cI", "HEAD"]);
    let installed_at = DateTime::parse_from_rfc3339(&date)
        .map_or_else(|_| Utc::now(), |value| value.with_timezone(&Utc));
    if output.exists() {
        fs::remove_dir_all(output)
            .map_err(|error| BuildError::io("remove previous release output", output, error))?;
    }
    fs::create_dir_all(output)
        .map_err(|error| BuildError::io("create release output", output, error))?;
    let ui_archive = output.join("sempre-ui.zip");
    sempre_build::prepare_ui(&root.join("ui/dist"), &ui_archive, &version)?;

    let environment = [
        ("SEMPRE_VERSION", version.as_str()),
        ("SEMPRE_COMMIT", commit.as_str()),
        ("SEMPRE_BUILD_DATE", date.as_str()),
    ];
    run_command(
        rust,
        "cargo",
        ["build", "--release", "-p", "sempre-client"],
        &environment,
    )?;
    let target = BuildTarget::current()?;
    if sempre_build::dns_capture_supported(&target) {
        let distribution =
            sempre_build::prepare_dns_capture(&rust.join("target/windivert")).await?;
        let library = distribution.join("x64").to_string_lossy().into_owned();
        let mut capture_environment = environment.to_vec();
        capture_environment.push(("WINDIVERT_PATH", library.as_str()));
        run_command(
            rust,
            "cargo",
            ["build", "--release", "-p", "sempre-dns-capture"],
            &capture_environment,
        )?;
        sempre_build::assemble_dns_capture(
            &rust.join("target/release/sempre-dns-capture.exe"),
            &distribution,
        )?;
    }
    let executable = rust.join("target/release").join(target.executable_name());
    let result = sempre_build::package(&BuildInput {
        executable,
        ui_archive,
        output: output.to_path_buf(),
        version,
        installed_at,
        target,
    })
    .await?;
    println!("Binary: {}", result.binary.display());
    println!("Bundle: {}", result.bundle.display());
    println!("UI: {}", result.ui_archive.display());
    println!("Checksums: {}", result.checksums.display());
    Ok(())
}

fn repository_root() -> Result<PathBuf, BuildError> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or_else(|| BuildError::Invalid("locate repository root".into()))?;
    if !root.join("package.json").is_file() || !root.join("rust/Cargo.toml").is_file() {
        return Err(BuildError::Invalid(format!(
            "invalid repository root {}",
            root.display()
        )));
    }
    Ok(root.to_path_buf())
}

fn release_version(explicit: Option<String>) -> String {
    explicit.unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned())
}

fn absolute_output(root: &Path, requested: &Path) -> Result<PathBuf, BuildError> {
    let output = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    if output == root || output == root.join("rust") || output.parent().is_none() {
        return Err(BuildError::Invalid(format!(
            "refuse unsafe release output {}",
            output.display()
        )));
    }
    Ok(output)
}

fn run_command<I, S>(
    directory: &Path,
    program: &str,
    arguments: I,
    environment: &[(&str, &str)],
) -> Result<(), BuildError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.current_dir(directory).args(arguments);
    for (name, value) in environment {
        command.env(name, value);
    }
    let status = command.status().map_err(|source| BuildError::Start {
        program: program.into(),
        source,
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(BuildError::Command {
            program: program.into(),
            code: status.code().unwrap_or(1),
        })
    }
}

fn git<I, S>(root: &Path, arguments: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("git")
        .current_dir(root)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| String::from("unknown"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_guard_rejects_repository_roots() {
        let root = Path::new("/workspace/sempre");
        assert!(absolute_output(root, root).is_err());
        assert!(absolute_output(root, Path::new("rust")).is_err());
        assert_eq!(
            absolute_output(root, Path::new("dist")).expect("dist"),
            root.join("dist")
        );
    }

    #[test]
    fn build_stages_are_explicit_and_default_to_all() {
        let all = Arguments::try_parse_from(["sempre-build"]).expect("default build");
        assert!(all.task.is_none());
        let verify = Arguments::try_parse_from(["sempre-build", "verify"]).expect("verify");
        assert!(matches!(verify.task, Some(Task::Verify)));
        let package = Arguments::try_parse_from([
            "sempre-build",
            "package",
            "--output",
            "artifacts",
            "--version",
            "v2.0.7",
        ])
        .expect("package");
        assert!(matches!(package.task, Some(Task::Package)));
        assert_eq!(package.output, PathBuf::from("artifacts"));
        assert_eq!(package.version.as_deref(), Some("v2.0.7"));
    }

    #[test]
    fn release_version_defaults_to_workspace_version_and_allows_tag_override() {
        assert_eq!(release_version(None), "2.0.7");
        assert_eq!(release_version(Some("v2.0.7".into())), "v2.0.7");
    }
}
