use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::VERSION;

#[derive(Debug, Parser)]
#[command(name = "sempre", version = VERSION, about = "Manage external proxy cores")]
pub(crate) struct Arguments {
    #[arg(long, conflicts_with = "portable", global = true)]
    pub system: bool,
    #[arg(long, conflicts_with = "system", global = true)]
    pub portable: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Run the authenticated local control daemon.
    Daemon {
        /// Override the persisted listen address for this process.
        #[arg(long)]
        listen: Option<String>,
        /// Use an isolated development data root with no native-service authority.
        #[arg(long, hide = true)]
        development_root: Option<PathBuf>,
    },
    /// Manage installed external proxy cores.
    Core {
        #[command(subcommand)]
        command: CoreCommand,
    },
    /// Export or restore a portable deployment snapshot.
    Bundle {
        #[command(subcommand)]
        command: BundleCommand,
    },
    /// Print build version information.
    Version,
    #[cfg(windows)]
    #[command(hide = true)]
    ServiceHost,
}

#[derive(Debug, Subcommand)]
pub(crate) enum BundleCommand {
    /// Export the current deployment to a ZIP archive in a directory.
    Export { directory: PathBuf },
    /// Restore this extracted snapshot as the system service.
    Restore {
        /// Replace a different existing system deployment.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum CoreCommand {
    /// Install an exact version or stable channel.
    Install { reference: String },
    /// List installed core versions.
    List,
    /// Select an installed core channel or exact version.
    Use { reference: String },
    /// Remove an unreferenced installed core version.
    Remove { reference: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_and_portable_modes_are_mutually_exclusive() {
        assert!(
            Arguments::try_parse_from(["sempre", "--system", "--portable", "version"]).is_err()
        );
    }

    #[test]
    fn parses_core_install_and_daemon_override() {
        let install = Arguments::try_parse_from([
            "sempre",
            "--portable",
            "core",
            "install",
            "sing-box@1.13.0",
        ])
        .expect("core install");
        assert!(matches!(
            install.command,
            Command::Core {
                command: CoreCommand::Install { .. }
            }
        ));
        let daemon = Arguments::try_parse_from([
            "sempre",
            "daemon",
            "--listen",
            "127.0.0.1:44000",
            "--development-root",
            ".cache/sempre-dev/runtime",
        ])
        .expect("daemon");
        assert!(matches!(
            daemon.command,
            Command::Daemon {
                listen: Some(_),
                development_root: Some(_)
            }
        ));
        let select = Arguments::try_parse_from(["sempre", "core", "use", "sing-box@stable"])
            .expect("core use");
        assert!(matches!(
            select.command,
            Command::Core {
                command: CoreCommand::Use { .. }
            }
        ));
        let restore = Arguments::try_parse_from(["sempre", "bundle", "restore", "--yes"])
            .expect("bundle restore");
        assert!(matches!(
            restore.command,
            Command::Bundle {
                command: BundleCommand::Restore { yes: true }
            }
        ));
    }
}
