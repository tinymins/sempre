use clap::{Subcommand, ValueEnum};

#[derive(Debug, Subcommand)]
pub(crate) enum RuntimeCommand {
    /// Print the managed runtime state.
    Status,
    /// Start the selected core and wait until it is running.
    Start,
    /// Stop the managed core and wait until it is stopped.
    Stop,
    /// Restart the managed core and wait for the replacement process.
    Restart,
    /// Schedule reconciliation without changing desired state.
    Reload,
    /// Print the capabilities exposed by the running core.
    Capabilities,
    /// Print core version, mode, traffic, and connection totals.
    Overview,
    /// Read or temporarily patch the running core configuration.
    Config {
        #[command(subcommand)]
        command: Option<RuntimeConfigCommand>,
    },
    /// Inspect proxy groups, select a proxy, or measure latency.
    Proxies {
        #[command(subcommand)]
        command: Option<RuntimeProxyCommand>,
    },
    /// Inspect and refresh proxy providers.
    Providers {
        #[command(subcommand)]
        command: Option<RuntimeProviderCommand>,
    },
    /// Print the running core rule list.
    Rules,
    /// Inspect and refresh rule providers.
    RuleProviders {
        #[command(subcommand)]
        command: Option<RuntimeRuleProviderCommand>,
    },
    /// Inspect or close active connections.
    Connections {
        #[command(subcommand)]
        command: Option<RuntimeConnectionCommand>,
    },
    /// Query the running core DNS resolver.
    Dns {
        #[command(subcommand)]
        command: RuntimeDnsCommand,
    },
    /// Manage volatile runtime caches.
    Cache {
        #[command(subcommand)]
        command: RuntimeCacheCommand,
    },
    /// Stream one core event topic as JSON lines.
    Events { topic: RuntimeStreamTopic },
    /// Stream traffic samples as JSON lines.
    Traffic,
    /// Stream memory samples as JSON lines.
    Memory,
    /// Stream core logs as JSON lines.
    Logs,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RuntimeConfigCommand {
    /// Patch one volatile core configuration field with a JSON value.
    Set { key: String, value: String },
}

#[derive(Debug, Subcommand)]
pub(crate) enum RuntimeProxyCommand {
    /// Select a proxy within a group.
    Select { group: String, proxy: String },
    /// Measure one proxy with an optional URL and timeout in milliseconds.
    Delay {
        name: String,
        url: Option<String>,
        timeout_ms: Option<u64>,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum RuntimeProviderCommand {
    /// Refresh a proxy provider.
    Update { name: String },
    /// Run a provider health check.
    Healthcheck { name: String },
}

#[derive(Debug, Subcommand)]
pub(crate) enum RuntimeRuleProviderCommand {
    /// Refresh a rule provider.
    Update { name: String },
}

#[derive(Debug, Subcommand)]
pub(crate) enum RuntimeConnectionCommand {
    /// Close one connection by ID, or every connection with --all.
    Close {
        #[arg(required_unless_present = "all", conflicts_with = "all")]
        id: Option<String>,
        #[arg(long, required_unless_present = "id", conflicts_with = "id")]
        all: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum RuntimeDnsCommand {
    /// Resolve a name with the running core.
    Query {
        name: String,
        #[arg(default_value = "A")]
        record_type: String,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum RuntimeCacheCommand {
    /// Flush the core fake-IP cache.
    Flush,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum RuntimeStreamTopic {
    Traffic,
    Memory,
    Connections,
    Logs,
}

impl RuntimeStreamTopic {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Traffic => "traffic",
            Self::Memory => "memory",
            Self::Connections => "connections",
            Self::Logs => "logs",
        }
    }
}
