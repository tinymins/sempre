use std::{ffi::OsString, io, process::Command};

use sempre_state::Mode;
use thiserror::Error;

use crate::args::Arguments;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Continue,
    Exit(i32),
}

#[derive(Debug, Error)]
pub enum ElevationError {
    #[error("locate current executable: {0}")]
    CurrentExecutable(#[source] io::Error),
    #[cfg(unix)]
    #[error("resolve current executable {path}: {source}")]
    ResolveExecutable {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("administrator access was not granted by {0}")]
    Denied(&'static str),
    #[error("run administrator access command {program}: {source}")]
    Start {
        program: &'static str,
        #[source]
        source: io::Error,
    },
    #[cfg(windows)]
    #[error("administrator status check failed with exit code {0}")]
    StatusCheck(i32),
    #[cfg(windows)]
    #[error("administrator access requires Unicode paths and arguments")]
    NonUnicode,
}

pub fn ensure(
    arguments: &Arguments,
    raw_arguments: &[OsString],
    mode: Mode,
) -> Result<Outcome, ElevationError> {
    if !arguments.requires_administrator(mode) {
        return Ok(Outcome::Continue);
    }
    platform_ensure(arguments.elevated, raw_arguments)
}

#[cfg(unix)]
fn platform_ensure(
    explicitly_elevated: bool,
    raw_arguments: &[OsString],
) -> Result<Outcome, ElevationError> {
    if nix::unistd::Uid::effective().is_root() {
        return Ok(Outcome::Continue);
    }
    if explicitly_elevated {
        return Err(ElevationError::Denied("sudo"));
    }
    let executable = std::env::current_exe().map_err(ElevationError::CurrentExecutable)?;
    let executable =
        std::fs::canonicalize(&executable).map_err(|source| ElevationError::ResolveExecutable {
            path: executable.display().to_string(),
            source,
        })?;
    let status = Command::new("sudo")
        .arg("--")
        .arg(executable)
        .arg("--elevated")
        .args(raw_arguments)
        .status()
        .map_err(|source| ElevationError::Start {
            program: "sudo",
            source,
        })?;
    Ok(Outcome::Exit(status.code().unwrap_or(1)))
}

#[cfg(windows)]
fn platform_ensure(
    explicitly_elevated: bool,
    raw_arguments: &[OsString],
) -> Result<Outcome, ElevationError> {
    if windows_is_elevated()? {
        return Ok(Outcome::Continue);
    }
    if explicitly_elevated {
        return Err(ElevationError::Denied("Windows UAC"));
    }
    let executable = std::env::current_exe().map_err(ElevationError::CurrentExecutable)?;
    let working_directory = executable.parent().ok_or(ElevationError::NonUnicode)?;
    let executable = executable.to_str().ok_or(ElevationError::NonUnicode)?;
    let working_directory = working_directory
        .to_str()
        .ok_or(ElevationError::NonUnicode)?;
    let mut elevated_arguments = vec![String::from("--elevated")];
    for argument in raw_arguments {
        elevated_arguments.push(
            argument
                .to_str()
                .ok_or(ElevationError::NonUnicode)?
                .to_owned(),
        );
    }
    let command_line = elevated_arguments
        .iter()
        .map(|argument| quote_windows_argument(argument))
        .collect::<Vec<_>>()
        .join(" ");
    let script = windows_elevation_script(executable, &command_line, working_directory);
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .map_err(|source| ElevationError::Start {
            program: "powershell.exe",
            source,
        })?;
    Ok(Outcome::Exit(status.code().unwrap_or(1)))
}

#[cfg(windows)]
fn windows_elevation_script(executable: &str, arguments: &str, working_directory: &str) -> String {
    // Start-Process -Wait also waits for the cleanup descendant. The original
    // executable must exit first so that descendant can remove the installation.
    format!(
        "$ErrorActionPreference = 'Stop'; $process = Start-Process -FilePath {} -ArgumentList {} -WorkingDirectory {} -Verb RunAs -PassThru; $null = $process.Handle; $process.WaitForExit(); exit $process.ExitCode",
        powershell_literal(executable),
        powershell_literal(arguments),
        powershell_literal(working_directory)
    )
}

#[cfg(windows)]
fn windows_is_elevated() -> Result<bool, ElevationError> {
    const CHECK: &str = "$identity = [Security.Principal.WindowsIdentity]::GetCurrent(); $principal = New-Object Security.Principal.WindowsPrincipal($identity); if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { exit 0 } else { exit 3 }";
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", CHECK])
        .status()
        .map_err(|source| ElevationError::Start {
            program: "powershell.exe",
            source,
        })?;
    match status.code() {
        Some(0) => Ok(true),
        Some(3) => Ok(false),
        Some(code) => Err(ElevationError::StatusCheck(code)),
        None => Err(ElevationError::StatusCheck(1)),
    }
}

#[cfg(any(windows, test))]
fn powershell_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(any(windows, test))]
fn quote_windows_argument(value: &str) -> String {
    if value.is_empty() {
        return String::from("\"\"");
    }
    if !value
        .chars()
        .any(|character| character.is_whitespace() || character == '\"')
    {
        return value.to_owned();
    }
    let mut quoted = String::from("\"");
    let mut backslashes = 0;
    for character in value.chars() {
        match character {
            '\\' => backslashes += 1,
            '\"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('\"');
                backslashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                quoted.push(character);
            }
        }
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('\"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_and_powershell_quoting_preserve_special_arguments() {
        assert_eq!(quote_windows_argument("plain"), "plain");
        assert_eq!(quote_windows_argument(""), r#""""#);
        assert_eq!(
            quote_windows_argument(r"C:\Program Files\Sempre\"),
            r#""C:\Program Files\Sempre\\""#
        );
        assert_eq!(quote_windows_argument(r#"a\"b"#), r#""a\\\"b""#);
        assert_eq!(powershell_literal("a'b"), "'a''b'");
    }
}

#[cfg(all(test, windows))]
mod windows_tests;
