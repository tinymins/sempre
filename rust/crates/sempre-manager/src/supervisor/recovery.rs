use std::{fs, path::Path, time::Duration};

use chrono::Utc;
use sempre_state::{DesiredState, RuntimeState};
use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, System};
use tokio::time::{Instant, sleep};

use crate::{Manager, ManagerError, ValidationRunner, VersionRunner};

const EXIT_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) async fn recover_stale_process<R: VersionRunner + ValidationRunner>(
    manager: &Manager<R>,
) -> Result<(), ManagerError> {
    let document = manager.store.read()?;
    let Some(pid) = document.runtime.pid else {
        return Ok(());
    };
    let mut system = System::new_all();
    let process = system.process(Pid::from_u32(pid));
    let expected = document.active.as_ref().map(|deployment| {
        manager.store.layout().core_binary(
            &deployment.core,
            deployment.repository.as_deref(),
            &deployment.version,
        )
    });
    let owned = process
        .and_then(sysinfo::Process::exe)
        .zip(expected.as_deref())
        .is_some_and(|(actual, expected)| same_executable(actual, expected));
    if owned {
        manager.log_supervisor(&format!(
            "terminating stale managed core PID {pid} after service restart"
        ))?;
        sempre_supervisor::terminate_tree(pid, true)
            .await
            .map_err(|error| ManagerError::io("terminate stale managed core", error))?;
        wait_for_exit(&mut system, pid).await?;
    } else if process.is_some() {
        manager.log_supervisor(&format!(
            "discarding stale runtime PID {pid}; executable does not match the active core"
        ))?;
    }
    manager.store.update(|document| {
        if document.runtime.pid == Some(pid) {
            document.runtime.pid = None;
            document.runtime.state = if document.desired_state == DesiredState::Running {
                RuntimeState::Restarting
            } else {
                RuntimeState::Stopped
            };
            document.runtime.last_exit = Some("recovered after Sempre service restart".into());
            document.runtime.last_transition = Some(Utc::now());
        }
        Ok(())
    })?;
    Ok(())
}

fn same_executable(actual: &Path, expected: &Path) -> bool {
    let actual = fs::canonicalize(actual).unwrap_or_else(|_| actual.to_path_buf());
    let expected = fs::canonicalize(expected).unwrap_or_else(|_| expected.to_path_buf());
    if cfg!(windows) {
        actual
            .to_string_lossy()
            .eq_ignore_ascii_case(&expected.to_string_lossy())
    } else {
        actual == expected
    }
}

async fn wait_for_exit(system: &mut System, pid: u32) -> Result<(), ManagerError> {
    let pid = Pid::from_u32(pid);
    let deadline = Instant::now() + EXIT_TIMEOUT;
    loop {
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        if system
            .process(pid)
            .is_none_or(|process| process.status() == ProcessStatus::Zombie)
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(ManagerError::io(
                "wait for stale managed core to exit",
                std::io::Error::new(std::io::ErrorKind::TimedOut, "process is still running"),
            ));
        }
        sleep(Duration::from_millis(50)).await;
    }
}
