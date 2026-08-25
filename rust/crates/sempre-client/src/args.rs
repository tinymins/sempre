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
    #[arg(long, hide = true, global = true)]
    pub elevated: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Install this extracted release as the native system service.
    Install {
        /// Replace a different existing system deployment without prompting.
        #[arg(long)]
        yes: bool,
        /// Select this bundled or downloadable core reference before installation.
        #[arg(long)]
        core: Option<String>,
        /// Configure the default profile with this subscription URL or raw content.
        #[arg(long, conflicts_with = "subscription_file")]
        subscription: Option<String>,
        /// Read the subscription URL or raw content from a small local file.
        #[arg(long, conflicts_with = "subscription")]
        subscription_file: Option<PathBuf>,
        /// Replace the bundled UI with an HTTPS ZIP URL or owner/repository reference.
        #[arg(long)]
        ui: Option<String>,
        /// Require this SHA-256 digest for the custom UI archive.
        #[arg(long, requires = "ui")]
        ui_sha256: Option<String>,
    },
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
    /// Manage the native Sempre service registration and lifecycle.
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    /// Print build version information.
    Version,
    #[cfg(windows)]
    #[command(hide = true)]
    ServiceHost,
}

impl Arguments {
    pub fn requires_administrator(&self) -> bool {
        let system = !self.portable;
        match &self.command {
            Command::Install { .. } => true,
            Command::Daemon {
                development_root, ..
            } => development_root.is_none() && system,
            Command::Core { .. } => system,
            Command::Bundle { command } => match command {
                BundleCommand::Export { .. } => system,
                BundleCommand::Restore { .. } => true,
            },
            Command::Service { command } => !matches!(command, ServiceCommand::Status),
            Command::Version => false,
            #[cfg(windows)]
            Command::ServiceHost => false,
        }
    }
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

#[derive(Debug, Subcommand)]
pub(crate) enum ServiceCommand {
    /// Install this extracted release as the native system service.
    Install {
        /// Replace a different existing system deployment without prompting.
        #[arg(long)]
        yes: bool,
    },
    /// Remove the native service registration while retaining Sempre data.
    Uninstall,
    /// Start the native service.
    Start,
    /// Stop the native service.
    Stop,
    /// Restart the native service.
    Restart,
    /// Print the native service state.
    Status,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_and_portable_modes_are_mutually_exclusive() {
        assert!(
            Arguments::try_parse_from(["sempre", "--system", "--portable", "version"]).is_err()
        );
        let status =
            Arguments::try_parse_from(["sempre", "service", "status"]).expect("service status");
        assert!(!status.requires_administrator());
        let restart =
            Arguments::try_parse_from(["sempre", "service", "restart"]).expect("service restart");
        assert!(restart.requires_administrator());
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
        let install =
            Arguments::try_parse_from(["sempre", "install", "--yes"]).expect("release install");
        assert!(install.requires_administrator());
        assert!(matches!(
            install.command,
            Command::Install { yes: true, .. }
        ));
        let configured = Arguments::try_parse_from([
            "sempre",
            "install",
            "--core=mihomo@stable",
            "--subscription-file=subscription.txt",
            "--ui=https://example.com/ui.zip",
            "--ui-sha256",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ])
        .expect("configured install");
        assert!(matches!(
            configured.command,
            Command::Install {
                core: Some(_),
                subscription_file: Some(_),
                ui: Some(_),
                ui_sha256: Some(_),
                ..
            }
        ));
        assert!(
            Arguments::try_parse_from([
                "sempre",
                "install",
                "--subscription=https://example.com/sub",
                "--subscription-file=subscription.txt",
            ])
            .is_err()
        );
    }

    #[test]
    fn administrator_boundary_matches_mutating_system_commands() {
        let version = Arguments::try_parse_from(["sempre", "version"]).expect("version");
        assert!(!version.requires_administrator());
        let portable_core = Arguments::try_parse_from(["sempre", "--portable", "core", "list"])
            .expect("portable core list");
        assert!(!portable_core.requires_administrator());
        let system_core =
            Arguments::try_parse_from(["sempre", "core", "list"]).expect("system core list");
        assert!(system_core.requires_administrator());
        let development = Arguments::try_parse_from([
            "sempre",
            "daemon",
            "--development-root",
            ".cache/sempre-dev/runtime",
        ])
        .expect("development daemon");
        assert!(!development.requires_administrator());
    }
}
