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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layout {
    pub mode: Mode,
    pub root: PathBuf,
    pub home: PathBuf,
    pub logs: PathBuf,
    pub runtime: PathBuf,
    pub state: PathBuf,
    pub web_config: PathBuf,
    pub traffic_history: PathBuf,
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
            root: root.to_path_buf(),
            home: home.to_path_buf(),
            logs: logs.to_path_buf(),
            runtime: runtime.to_path_buf(),
            state: home.join("state.json"),
            web_config: home.join("web.json"),
            traffic_history: home.join("traffic-history.json"),
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

    fn directories(&self) -> [&Path; 14] {
        [
            &self.home,
            &self.logs,
            &self.runtime,
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
    fn isolated_layout_keeps_runtime_and_data_under_root() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let layout = Layout::at(temporary.path());
        assert_eq!(layout.mode, Mode::Portable);
        assert_eq!(layout.home, temporary.path().join(".sempre"));
        assert_eq!(layout.runtime, layout.home.join("run"));
    }

    #[test]
    fn development_layout_is_isolated_and_explicit() {
        let root = Path::new("/tmp/sempre-development");
        let layout = Layout::development_at(root);
        assert_eq!(layout.mode, Mode::Development);
        assert_eq!(layout.root, root);
        assert_eq!(layout.home, root.join(".sempre"));
        assert_eq!(layout.instance_lock, root.join(".sempre/run/instance.lock"));
    }

    #[test]
    fn system_layout_separates_managed_binary_from_command_registration() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let layout = Layout::system_at(temporary.path());
        assert_ne!(layout.service_executable, layout.command_executable);
        assert_eq!(
            layout.command_executable,
            temporary
                .path()
                .join("command")
                .join(executable_name("sempre"))
        );
    }

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
    fn ensure_creates_private_directories() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let layout = Layout::at(temporary.path());
        layout.ensure().expect("ensure layout");
        assert!(layout.subscription_cache.is_dir());
        assert!(layout.tunnel_logs.is_dir());
    }

    #[test]
    fn portable_marker_selects_mode_and_changes_idempotently() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let executable = temporary.path().join(executable_name("sempre"));
        assert_eq!(
            Mode::for_executable(&executable).expect("system mode"),
            Mode::System
        );
        let marker = set_portable_marker(&executable, true).expect("enable portable marker");
        set_portable_marker(&executable, true).expect("enable portable marker twice");
        assert_eq!(marker, temporary.path().join(PORTABLE_MARKER));
        assert_eq!(
            Mode::for_executable(&executable).expect("portable mode"),
            Mode::Portable
        );
        set_portable_marker(&executable, false).expect("disable portable marker");
        set_portable_marker(&executable, false).expect("disable portable marker twice");
        assert_eq!(
            Mode::for_executable(&executable).expect("system mode"),
            Mode::System
        );
    }
}
