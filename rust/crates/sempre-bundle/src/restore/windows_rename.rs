use std::{
    fs, io,
    path::Path,
    thread,
    time::{Duration, Instant},
};

pub(super) fn rename_with_retry(source: &Path, target: &Path) -> io::Result<()> {
    rename_for(source, target, Duration::from_secs(30))
}

fn rename_for(source: &Path, target: &Path, timeout: Duration) -> io::Result<()> {
    let started = Instant::now();
    loop {
        match fs::rename(source, target) {
            Ok(()) => return Ok(()),
            // A handle without FILE_SHARE_DELETE can block a file or its parent directory.
            Err(error)
                if matches!(error.raw_os_error(), Some(5 | 32 | 33))
                    && started.elapsed() < timeout =>
            {
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt as _};

    use super::*;

    #[test]
    fn activation_waits_for_a_directory_lock_longer_than_two_seconds() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = temporary.path().join(".cores.sempre-stage");
        let target = temporary.path().join("cores");
        fs::create_dir_all(&source).expect("source directory");
        let executable = source.join("core.exe");
        fs::write(&executable, b"executable").expect("source executable");
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&executable)
            .expect("exclusive file handle");
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_secs(3));
            drop(locked);
        });

        rename_with_retry(&source, &target).expect("retry directory activation");
        release.join().expect("release file handle");
        assert_eq!(fs::read(target.join("core.exe")).unwrap(), b"executable");
        assert!(!source.exists());
    }

    #[test]
    fn replacement_waits_for_a_destination_file_handle() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = temporary.path().join("staged.exe");
        let target = temporary.path().join("target.exe");
        fs::write(&source, b"new executable").expect("source executable");
        fs::write(&target, b"old executable").expect("target executable");
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&target)
            .expect("exclusive file handle");
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            drop(locked);
        });

        rename_with_retry(&source, &target).expect("retry file replacement");
        release.join().expect("release file handle");
        assert_eq!(fs::read(&target).unwrap(), b"new executable");
    }

    #[test]
    fn persistent_lock_returns_the_error_and_preserves_both_files() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = temporary.path().join("staged.exe");
        let target = temporary.path().join("target.exe");
        fs::write(&source, b"new executable").expect("source executable");
        fs::write(&target, b"old executable").expect("target executable");
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&target)
            .expect("exclusive file handle");
        let error = rename_for(&source, &target, Duration::from_millis(100))
            .expect_err("persistent lock must fail");
        assert!(matches!(error.raw_os_error(), Some(5 | 32 | 33)));
        drop(locked);
        assert_eq!(fs::read(&source).unwrap(), b"new executable");
        assert_eq!(fs::read(&target).unwrap(), b"old executable");
    }

    #[test]
    fn missing_source_is_not_retried() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let started = Instant::now();
        let error = rename_with_retry(
            &temporary.path().join("missing"),
            &temporary.path().join("target"),
        )
        .expect_err("missing source");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
