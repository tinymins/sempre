use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::{VERSION, runtime_args::RuntimeCommand};

#[derive(Debug, Parser)]
#[command(name = "sempre", version = VERSION, about = "Manage external proxy cores")]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Arguments {
    #[arg(long, conflicts_with = "portable", global = true)]
    pub system: bool,
    #[arg(long, conflicts_with = "system", global = true)]
    pub portable: bool,
    #[arg(long, hide = true, global = true)]
    pub elevated: bool,
    /// Print supported command output as JSON.
    #[arg(long, global = true)]
    pub json: bool,
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
    /// Remove the system application while retaining data unless explicitly purged.
    Uninstall {
        /// Remove configuration, subscriptions, passwords, and all other Sempre data.
        #[arg(long)]
        purge: bool,
        /// Proceed without an interactive confirmation.
        #[arg(long)]
        yes: bool,
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
    /// Import subscription input for the active profile.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Manage local and remote subscription profiles.
    Subscription {
        #[command(subcommand)]
        command: SubscriptionCommand,
    },
    /// Manage reusable custom proxy nodes.
    CustomNode {
        #[command(subcommand)]
        command: CustomNodeCommand,
    },
    /// Inspect and change the authenticated Web listener.
    Web {
        #[command(subcommand)]
        command: WebCommand,
    },
    /// Install and manage the control UI.
    Ui {
        #[command(subcommand)]
        command: UiCommand,
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
    /// Inspect and control the managed proxy-core runtime.
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
    },
    /// Refresh, compile, validate, and stage the active subscription profile.
    Update,
    /// Print deployment, service, runtime, and subscription status.
    Status,
    /// Print manager and managed-core logs.
    Logs {
        /// Continue printing appended log records until interrupted.
        #[arg(long)]
        follow: bool,
    },
    /// Run installation, core, configuration, runtime, and network diagnostics.
    Doctor,
    /// Open the authenticated control UI in the default browser.
    Open,
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
            Command::Uninstall { .. } => true,
            Command::Daemon {
                development_root, ..
            } => development_root.is_none(),
            Command::Core { .. } => system,
            Command::Config { .. } => system,
            Command::Subscription { .. } => system,
            Command::CustomNode { .. } => system,
            Command::Web { .. } | Command::Ui { .. } => system,
            Command::Bundle { command } => match command {
                BundleCommand::Export { .. } => system,
                BundleCommand::Restore { .. } => true,
            },
            Command::Service { command } => !matches!(command, ServiceCommand::Status),
            Command::Runtime { .. } => system,
            Command::Update => system,
            Command::Status | Command::Logs { .. } | Command::Doctor | Command::Open => system,
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
    /// Update all installed channels or one explicit channel reference.
    Update { reference: Option<String> },
    /// List installed core versions.
    List,
    /// Print the selected and active core deployments.
    Current,
    /// Select an installed core channel or exact version.
    Use { reference: String },
    /// Remove an unreferenced installed core version.
    Remove { reference: String },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigCommand {
    /// Add a local file as a raw source and stage its converted configuration.
    Import { file: PathBuf },
}

#[derive(Debug, Subcommand)]
pub(crate) enum SubscriptionCommand {
    /// List subscription profiles.
    List,
    /// Print one profile, or the active profile when omitted.
    Show { id: Option<String> },
    /// Create a local profile.
    Create { name: String },
    /// Create a read-only remote profile from a Sempre server manifest.
    CreateRemote { name: String, manifest_url: String },
    /// Replace a local profile from a JSON file without compiling it.
    Save { id: String, file: PathBuf },
    /// Activate, fetch, compile, validate, and stage a profile.
    Use { id: String },
    /// Refresh, compile, and validate a profile.
    Update { id: Option<String> },
    /// Render a profile without changing runtime state.
    Render {
        id: Option<String>,
        #[arg(long, default_value = "clash-meta")]
        format: String,
    },
    /// Remove a non-active profile.
    Remove { id: String },
    /// Replace the active profile sources with one HTTP(S) URL; empty clears sources.
    Set { url: String },
    /// Set the scheduled refresh interval or disable it with `off`.
    Schedule { interval: String },
    /// Enable or disable automatic restart after scheduled refresh.
    AutoRestart { enabled: bool },
    /// Print active profile and refresh state.
    Status,
    /// Remove cached remote subscription responses.
    ClearCache,
    /// Add, remove, or test subscription sources.
    Source {
        #[command(subcommand)]
        command: SubscriptionSourceCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum SubscriptionSourceCommand {
    /// Append an HTTP(S) source to the active local profile.
    AddUrl { url: String },
    /// Append a raw UTF-8 source file to the active local profile.
    AddRaw { file: PathBuf },
    /// Remove a source from the active local profile.
    Remove { id: String },
    /// Fetch and parse an HTTP(S) source or parse a local UTF-8 file.
    Test { input: String },
}

#[derive(Debug, Subcommand)]
pub(crate) enum CustomNodeCommand {
    /// List saved custom nodes.
    List,
    /// Create a custom node from a `CustomNode` or proxy JSON file.
    Add { file: PathBuf },
    /// Replace a custom node from a `CustomNode` or proxy JSON file.
    Update { id: String, file: PathBuf },
    /// Remove an unreferenced custom node.
    Remove { id: String },
}

#[derive(Debug, Subcommand)]
pub(crate) enum WebCommand {
    /// Print the listener URL and password state.
    Status,
    /// Persist or live-rebind the Web listener.
    Listen { address: String },
    /// Manage the administrator password.
    Password {
        #[command(subcommand)]
        command: WebPasswordCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum WebPasswordCommand {
    /// Read and set a password from standard input.
    Set {
        #[arg(long, required = true)]
        stdin: bool,
    },
    /// Clear the password and allow same-origin empty-password login.
    Clear,
}

#[derive(Debug, Subcommand)]
pub(crate) enum UiCommand {
    /// Print installed UI metadata.
    Status,
    /// Install official, GitHub, HTTPS, or local ZIP UI content.
    Install {
        source: String,
        #[arg(long)]
        sha256: Option<String>,
    },
    /// Update the UI from its recorded non-local source.
    Update,
    /// Remove the installed UI.
    Remove,
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
#[path = "args_tests.rs"]
mod tests;
