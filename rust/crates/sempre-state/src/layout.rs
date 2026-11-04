use std::{
    env, fs,
    fs::OpenOptions,
    io,
    path::{Path, PathBuf},
};

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    System,
    Portable,
    Development,
}

pub const PORTABLE_MARKER: &str = ".sempre-portable";
const OPENWRT_RELEASE: &str = "/etc/openwrt_release";

/// Whether this host runs `OpenWrt`; the one place that decides the question.
pub fn openwrt_host() -> bool {
    cfg!(target_os = "linux") && Path::new(OPENWRT_RELEASE).exists()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layout {
    pub mode: Mode,
    pub openwrt: bool,
    pub root: PathBuf,
    pub home: PathBuf,
    pub logs: PathBuf,
    pub runtime: PathBuf,
    pub core_data: PathBuf,
    pub capture_release: PathBuf,
    pub capture_settings: PathBuf,
    pub capture_cn_list: PathBuf,
    pub state: PathBuf,
    pub web_config: PathBuf,
    pub traffic_history: PathBuf,
    pub dns_settings: PathBuf,
    pub dns_query_history: PathBuf,
    pub network_settings: PathBuf,
    pub ui: PathBuf,
    pub resources: PathBuf,
    pub endpoint: PathBuf,
    pub daemon_control: PathBuf,
    pub core_control: PathBuf,
    pub state_lock: PathBuf,
    pub operation_lock: PathBuf,
    pub config_lock: PathBuf,
    pub instance_lock: PathBuf,
    pub cores: PathBuf,
    pub configs: PathBuf,
    pub subscriptions: PathBuf,
    pub subscription_catalog: PathBuf,
    pub subscription_blobs: PathBuf,
    pub subscription_cache: PathBuf,
    pub subscription_lock: PathBuf,
    pub gateway: PathBuf,
    pub gateway_rules: PathBuf,
    pub dns_frontend: PathBuf,
    pub tunnels: PathBuf,
    pub tunnel_runtime: PathBuf,
    pub tunnel_logs: PathBuf,
    pub tools: PathBuf,
    pub manager_log: PathBuf,
    pub core_stdout_log: PathBuf,
    pub core_stderr_log: PathBuf,
    pub service_executable: PathBuf,
    pub command_executable: PathBuf,
}

#[derive(Debug, Error)]
pub enum LayoutError {
    #[error("locate current executable: {0}")]
    CurrentExecutable(#[source] io::Error),
    #[error("required environment variable {0} is not set")]
    MissingEnvironment(&'static str),
    #[error("development mode requires an explicit isolated root")]
    DevelopmentRootRequired,
    #[error("create layout directory {path}: {source}")]
    CreateDirectory { path: PathBuf, source: io::Error },
    #[error("{operation} portable marker {path}: {source}")]
    PortableMarker {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl Mode {
    pub fn current() -> Result<Self, LayoutError> {
        let executable = env::current_exe().map_err(LayoutError::CurrentExecutable)?;
        Self::for_executable(&executable)
    }

    pub fn for_executable(executable: &Path) -> Result<Self, LayoutError> {
        if portable_marker_enabled(executable)? {
            Ok(Self::Portable)
        } else {
            Ok(Self::System)
        }
    }
}

pub fn portable_marker_path(executable: &Path) -> PathBuf {
    executable
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(PORTABLE_MARKER)
}

pub fn portable_marker_enabled(executable: &Path) -> Result<bool, LayoutError> {
    let path = portable_marker_path(executable);
    match fs::metadata(&path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(LayoutError::PortableMarker {
            operation: "inspect",
            path,
            source,
        }),
    }
}

pub fn set_portable_marker(executable: &Path, enabled: bool) -> Result<PathBuf, LayoutError> {
    let path = portable_marker_path(executable);
    if enabled {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(path),
            Err(source) => Err(LayoutError::PortableMarker {
                operation: "create",
                path,
                source,
            }),
        }
    } else {
        match fs::remove_file(&path) {
            Ok(()) => Ok(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(path),
            Err(source) => Err(LayoutError::PortableMarker {
                operation: "remove",
                path,
                source,
            }),
        }
    }
}

impl Layout {
    pub fn for_mode(mode: Mode) -> Result<Self, LayoutError> {
        match mode {
            Mode::System => Self::system(),
            Mode::Portable => {
                let executable = env::current_exe().map_err(LayoutError::CurrentExecutable)?;
                let mut layout = Self::portable_at(&executable);
                layout.instance_lock = Self::system()?.instance_lock;
                Ok(layout)
            }
            Mode::Development => Err(LayoutError::DevelopmentRootRequired),
        }
    }

    pub fn development_at(root: &Path) -> Self {
        let executable = root.join(executable_name("sempre"));
        let home = root.join(".sempre");
        Self::new(
            Mode::Development,
            root,
            &home,
            &home.join("logs"),
            &home.join("run"),
            &executable,
        )
    }

    pub fn portable_at(executable: &Path) -> Self {
        let root = executable.parent().unwrap_or_else(|| Path::new("."));
        let home = root.join(".sempre");
        Self::new(
            Mode::Portable,
            root,
            &home,
            &home.join("logs"),
            &home.join("run"),
            executable,
        )
    }

    pub fn at(root: &Path) -> Self {
        Self::portable_at(&root.join(executable_name("sempre")))
    }

    pub fn system_at(root: &Path) -> Self {
        let binary_root = root.join("bin");
        let executable = binary_root.join(executable_name("sempre"));
        let mut layout = Self::new(
            Mode::System,
            &binary_root,
            &root.join("data"),
            &root.join("logs"),
            &root.join("run"),
            &executable,
        );
        layout.command_executable = root.join("command").join(executable_name("sempre"));
        layout
    }

    /// Keeps configuration on flash and everything that churns in RAM.
    ///
    /// `/var` is tmpfs on `OpenWrt`, so state cannot use the generic Linux paths. The core
    /// data directory stays on flash because its cache holds node selection, `FakeIP`
    /// mappings, and the remote rule sets needed to start before the WAN is up.
    pub fn openwrt_at(prefix: &Path) -> Self {
        let binary_root = prefix.join("usr/libexec/sempre");
        let home = prefix.join("etc/sempre");
        let volatile = prefix.join("tmp/sempre");
        let runtime = volatile.join("run");
        let mut layout = Self::new(
            Mode::System,
            &binary_root,
            &home,
            &volatile.join("logs"),
            &runtime,
            &binary_root.join("sempre"),
        );
        layout.openwrt = true;
        layout.core_data = home.join("core-data");
        layout.endpoint = runtime.join("endpoint.json");
        layout.traffic_history = runtime.join("traffic-history.json");
        layout.dns_query_history = runtime.join("dns-query-history.json");
        layout.command_executable = prefix.join("usr/bin/sempre");
        layout
    }

    pub fn ensure(&self) -> Result<(), LayoutError> {
        for directory in self.directories() {
            fs::create_dir_all(directory).map_err(|source| LayoutError::CreateDirectory {
                path: directory.to_path_buf(),
                source,
            })?;
            secure_private_directory(directory)?;
        }
        Ok(())
    }

    /// Records that the daemon must withdraw transparent capture when it next exits.
    pub fn request_capture_release(&self) -> Result<(), LayoutError> {
        self.ensure()?;
        fs::write(&self.capture_release, b"").map_err(|source| LayoutError::CreateDirectory {
            path: self.capture_release.clone(),
            source,
        })
    }

    /// Consumes a pending release request, reporting whether one was made.
    pub fn take_capture_release(&self) -> bool {
        fs::remove_file(&self.capture_release).is_ok()
    }

    pub fn ensure_instance_lock_directory(&self) -> Result<(), LayoutError> {
        let directory = self
            .instance_lock
            .parent()
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(directory).map_err(|source| LayoutError::CreateDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
        secure_private_directory(directory)
    }

    pub fn core_version_dir(&self, core: &str, repository: Option<&str>, version: &str) -> PathBuf {
        match repository.filter(|value| !value.is_empty()) {
            Some(repository) => self
                .cores
                .join(core)
                .join("sources")
                .join(repository)
                .join(version),
            None => self.cores.join(core).join(version),
        }
    }

    pub fn core_binary(&self, core: &str, repository: Option<&str>, version: &str) -> PathBuf {
        self.core_version_dir(core, repository, version)
            .join(executable_name(core))
    }

    pub fn config(&self, core: &str, hash: &str) -> PathBuf {
        self.configs.join(core).join(format!("{hash}.json"))
    }

    pub fn dns_frontend_policy(&self, hash: &str) -> PathBuf {
        self.dns_frontend.join(format!("{hash}.json"))
    }

    fn system() -> Result<Self, LayoutError> {
        #[cfg(target_os = "linux")]
        {
            if openwrt_host() {
                return Ok(Self::openwrt_at(Path::new("/")));
            }
            let mut layout = Self::new(
                Mode::System,
                Path::new("/usr/local/libexec/sempre"),
                Path::new("/var/lib/sempre"),
                Path::new("/var/log/sempre"),
                Path::new("/run/sempre"),
                Path::new("/usr/local/libexec/sempre/sempre"),
            );
            layout.command_executable = PathBuf::from("/usr/local/bin/sempre");
            return Ok(layout);
        }

        #[cfg(target_os = "macos")]
        {
            let mut layout = Self::new(
                Mode::System,
                Path::new("/Library/Application Support/Sempre/bin"),
                Path::new("/Library/Application Support/Sempre/data"),
                Path::new("/Library/Logs/Sempre"),
                Path::new("/var/run/sempre"),
                Path::new("/Library/Application Support/Sempre/bin/sempre"),
            );
            layout.command_executable = PathBuf::from("/usr/local/bin/sempre");
            return Ok(layout);
        }

        #[cfg(target_os = "windows")]
        {
            let program_files = env::var_os("ProgramFiles")
                .ok_or(LayoutError::MissingEnvironment("ProgramFiles"))?;
            let program_data =
                env::var_os("ProgramData").ok_or(LayoutError::MissingEnvironment("ProgramData"))?;
            let root = PathBuf::from(program_files).join("Sempre");
            let home = PathBuf::from(program_data).join("Sempre");
            return Ok(Self::new(
                Mode::System,
                &root,
                &home,
                &home.join("logs"),
                &home.join("run"),
                &root.join("sempre.exe"),
            ));
        }

        #[allow(unreachable_code)]
        Err(LayoutError::MissingEnvironment(
            "supported operating system",
        ))
    }

    fn new(
        mode: Mode,
        root: &Path,
        home: &Path,
        logs: &Path,
        runtime: &Path,
        executable: &Path,
    ) -> Self {
        let subscriptions = home.join("subscriptions");
        let gateway = home.join("gateway");
        let tunnels = home.join("tunnels.json");
        Self {
            mode,
            openwrt: false,
            root: root.to_path_buf(),
            home: home.to_path_buf(),
            logs: logs.to_path_buf(),
            runtime: runtime.to_path_buf(),
            core_data: runtime.to_path_buf(),
            capture_release: runtime.join("capture-release"),
            capture_settings: home.join("capture.json"),
            capture_cn_list: home.join("cn_ip.txt"),
            state: home.join("state.json"),
            web_config: home.join("web.json"),
            traffic_history: home.join("traffic-history.json"),
            dns_settings: home.join("dns.json"),
            dns_query_history: home.join("dns-query-history.json"),
            network_settings: home.join("network.json"),
            ui: home.join("ui"),
            resources: root.join("resources"),
            endpoint: root.join("endpoint.json"),
            daemon_control: runtime.join("sempre-control.json"),
            core_control: runtime.join("control.json"),
            state_lock: runtime.join("state.lock"),
            operation_lock: runtime.join("operation.lock"),
            config_lock: runtime.join("config.lock"),
            instance_lock: runtime.join("instance.lock"),
            cores: home.join("cores"),
            configs: home.join("configs"),
            subscription_catalog: subscriptions.join("catalog.json"),
            subscription_blobs: subscriptions.join("blobs"),
            subscription_cache: subscriptions.join("cache"),
            subscription_lock: runtime.join("subscription.lock"),
            subscriptions,
            gateway_rules: gateway.join("rules"),
            gateway,
            dns_frontend: home.join("dns-frontend"),
            tunnels,
            tunnel_runtime: runtime.join("tunnels"),
            tunnel_logs: logs.join("tunnels"),
            tools: home.join("tools"),
            manager_log: logs.join("sempre.log"),
            core_stdout_log: logs.join("core.stdout.log"),
            core_stderr_log: logs.join("core.stderr.log"),
            service_executable: executable.to_path_buf(),
            command_executable: executable.to_path_buf(),
        }
    }

    fn directories(&self) -> [&Path; 15] {
        [
            &self.home,
            &self.logs,
            &self.runtime,
            &self.core_data,
            &self.cores,
            &self.configs,
            &self.subscriptions,
            &self.subscription_blobs,
            &self.subscription_cache,
            &self.gateway,
            &self.gateway_rules,
            &self.dns_frontend,
            &self.tools,
            &self.tunnel_runtime,
            &self.tunnel_logs,
        ]
    }
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

#[cfg(unix)]
fn secure_private_directory(path: &Path) -> Result<(), LayoutError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
        LayoutError::CreateDirectory {
            path: path.to_path_buf(),
            source,
        }
    })
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn secure_private_directory(_path: &Path) -> Result<(), LayoutError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_core_sources_do_not_share_version_directories() {
        let layout = Layout::at(Path::new("sandbox"));
        let official = layout.core_binary("sing-box", None, "1.2.3");
        let custom = layout.core_binary("sing-box", Some("tinymins/sing-box"), "1.2.3");
        assert_ne!(official, custom);
        let expected =
            Path::new("sing-box/sources/tinymins/sing-box/1.2.3").join(executable_name("sing-box"));
        assert!(custom.ends_with(expected));
    }

    #[test]
    fn openwrt_keeps_configuration_on_flash_and_churn_in_ram() {
        let layout = Layout::openwrt_at(Path::new("/"));
        assert!(layout.openwrt);
        for persistent in [
            &layout.state,
            &layout.subscription_catalog,
            &layout.configs,
            &layout.core_data,
            &layout.ui,
        ] {
            assert!(persistent.starts_with("/etc/sempre"), "{persistent:?}");
        }
        for volatile in [
            &layout.runtime,
            &layout.logs,
            &layout.manager_log,
            &layout.endpoint,
            &layout.traffic_history,
            &layout.dns_query_history,
            &layout.instance_lock,
        ] {
            assert!(volatile.starts_with("/tmp/sempre"), "{volatile:?}");
        }
    }

    #[test]
    fn other_hosts_keep_core_data_in_the_runtime_directory() {
        let layout = Layout::at(Path::new("sandbox"));
        assert!(!layout.openwrt);
        assert_eq!(layout.core_data, layout.runtime);
    }
}
