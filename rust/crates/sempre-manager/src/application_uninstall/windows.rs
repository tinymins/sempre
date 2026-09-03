use std::{path::Path, process::Command};

use super::{ManagerError, remove_tree};

pub(super) fn remove_installation_root(path: &Path) -> Result<bool, ManagerError> {
    let executable = std::env::current_exe()
        .map_err(|error| ManagerError::io("locate current executable", error))?;
    let executable_parent = executable.parent().unwrap_or_else(|| Path::new(""));
    if !executable_parent
        .to_string_lossy()
        .eq_ignore_ascii_case(&path.to_string_lossy())
    {
        remove_tree(path).map_err(|error| {
            ManagerError::io(
                format!("remove installation directory {}", path.display()),
                error,
            )
        })?;
        return Ok(false);
    }
    removal_command(path, std::process::id())
        .spawn()
        .map_err(|error| ManagerError::io("schedule installation removal", error))?;
    Ok(true)
}

fn removal_command(path: &Path, parent_id: u32) -> Command {
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            include_str!("windows_remove.ps1"),
        ])
        // Keep paths out of shell syntax and the helper's working directory out
        // of the installation, including when uninstall was invoked from there.
        .env("SEMPRE_UNINSTALL_ROOT", path)
        .env("SEMPRE_UNINSTALL_PID", parent_id.to_string())
        .current_dir(std::env::temp_dir());
    command
}

#[cfg(test)]
mod tests {
    use std::{fs, os::windows::fs::OpenOptionsExt as _, thread, time::Duration};

    use super::*;

    const CHILD_TEST: &str =
        "application_uninstall::windows::tests::uninstall_from_installed_executable";

    #[test]
    fn uninstall_from_installed_executable() {
        let Some(root) = std::env::var_os("SEMPRE_UNINSTALL_TEST_ROOT") else {
            return;
        };
        let root = Path::new(&root);
        assert!(remove_installation_root(root).expect("schedule removal"));
        // Longer than the old fixed ping delay: the helper must wait for us.
        thread::sleep(Duration::from_secs(3));
        assert!(root.join("resources/marker").exists());
    }

    #[test]
    fn self_uninstall_waits_and_removes_literal_paths_from_installation_cwd() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        for name in [
            "Program Files Sempre",
            "Program Files 空格 & % ! ' [Sempre]",
        ] {
            let root = temporary.path().join(name);
            fs::create_dir_all(root.join("resources")).expect("resources");
            fs::write(root.join("resources/marker"), b"resource").expect("resource");
            let executable = root.join("sempre.exe");
            fs::copy(
                std::env::current_exe().expect("test executable"),
                &executable,
            )
            .expect("installed executable");
            let output = Command::new(&executable)
                .args(["--exact", CHILD_TEST, "--nocapture"])
                .env("SEMPRE_UNINSTALL_TEST_ROOT", &root)
                .current_dir(&root)
                .output()
                .expect("run installed executable");
            assert!(output.status.success(), "{output:?}");
            assert!(!root.exists(), "{output:?}");
            assert!(
                String::from_utf8_lossy(&output.stdout).contains("Installation directory removed.")
            );
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }

    #[test]
    fn removal_reports_failure_when_files_remain_locked() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let root = temporary.path().join("Sempre");
        fs::create_dir(&root).expect("installation directory");
        let _locked = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(root.join("locked.exe"))
            .expect("locked executable");
        // A terminated process models an uninstaller which has already exited.
        let mut parent = Command::new("cmd.exe")
            .args(["/D", "/C", "exit 0"])
            .spawn()
            .expect("parent process");
        let parent_id = parent.id();
        parent.wait().expect("parent exit");
        let output = removal_command(&root, parent_id)
            .output()
            .expect("cleanup process");
        assert!(!output.status.success(), "{output:?}");
        assert!(root.join("locked.exe").exists());
        assert!(String::from_utf8_lossy(&output.stderr).contains("installation removal failed"));
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("Installation directory removed.")
        );
    }
}
