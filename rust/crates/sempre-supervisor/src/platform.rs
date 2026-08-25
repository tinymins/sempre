use std::io;

#[cfg(not(windows))]
use std::future::Future;

use tokio::process::Command;

#[cfg(unix)]
pub fn configure(command: &mut Command) {
    use std::os::unix::process::CommandExt as _;

    command.as_std_mut().process_group(0);
}

#[cfg(windows)]
pub fn configure(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command
        .as_std_mut()
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
}

#[cfg(not(any(unix, windows)))]
pub fn configure(_: &mut Command) {}

#[cfg(unix)]
pub fn terminate_tree(pid: u32, force: bool) -> impl Future<Output = io::Result<()>> + Send {
    use nix::{
        errno::Errno,
        sys::signal::{Signal, killpg},
        unistd::Pid,
    };

    let result = i32::try_from(pid)
        .map_err(io::Error::other)
        .and_then(|pid| {
            let signal = if force {
                Signal::SIGKILL
            } else {
                Signal::SIGTERM
            };
            match killpg(Pid::from_raw(pid), signal) {
                Ok(()) | Err(Errno::ESRCH) => Ok(()),
                Err(error) => Err(io::Error::other(error)),
            }
        });
    std::future::ready(result)
}

#[cfg(windows)]
pub async fn terminate_tree(pid: u32, force: bool) -> io::Result<()> {
    let mut command = Command::new("taskkill.exe");
    command.args(["/PID", &pid.to_string(), "/T"]);
    if force {
        command.arg("/F");
    }
    let output = command.output().await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

#[cfg(not(any(unix, windows)))]
pub fn terminate_tree(_: u32, _: bool) -> impl Future<Output = io::Result<()>> + Send {
    std::future::ready(Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "process tree termination is unsupported",
    )))
}
