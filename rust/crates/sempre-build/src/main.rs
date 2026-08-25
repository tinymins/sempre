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
    /// Replace the Git-derived release version.
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
    let output = absolute_output(&root, &arguments.output)?;
    if !matches!(arguments.task, Some(Task::Package)) {
        verify(&root, &rust)?;
    }
    if matches!(arguments.task, Some(Task::Verify)) {
        return Ok(());
    }
    build_release(&root, &rust, &output, arguments.version).await
}

fn verify(root: &Path, rust: &Path) -> Result<(), BuildError> {
    run_command(rust, "cargo", ["fmt", "--all", "--", "--check"], &[])?;
    run_command(rust, "cargo", ["test", "--workspace"], &[])?;
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
        &[],
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

    let version =
        version.unwrap_or_else(|| git(root, ["describe", "--tags", "--always", "--dirty"]));
    let version = if version == "unknown" {
        String::from("dev")
    } else {
        version
    };
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
            "v2.0.0",
        ])
        .expect("package");
        assert!(matches!(package.task, Some(Task::Package)));
        assert_eq!(package.output, PathBuf::from("artifacts"));
        assert_eq!(package.version.as_deref(), Some("v2.0.0"));
    }
}
