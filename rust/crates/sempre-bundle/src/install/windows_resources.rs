use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    BundleError,
    restore::{RestoreTransaction, Swap},
};

pub(super) fn stage(
    source: &Path,
    target: &Path,
    transaction: &mut RestoreTransaction,
) -> Result<(), BundleError> {
    let mut paths = BTreeSet::new();
    collect(source, source, &mut paths)?;
    collect(target, target, &mut paths)?;
    for relative in paths {
        let from = source.join(&relative);
        let to = target.join(relative);
        // Loaded drivers can prevent renaming their parent directory even when
        // their bytes do not change. Leave identical resources in place.
        if from.is_file() && to.is_file() && read(&from)? == read(&to)? {
            continue;
        }
        transaction
            .operations
            .push(Swap::stage(&from, &to, false, false)?);
    }
    Ok(())
}

fn collect(root: &Path, path: &Path, paths: &mut BTreeSet<PathBuf>) -> Result<(), BundleError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(BundleError::Io {
                operation: "inspect resource",
                path: path.into(),
                source,
            });
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(BundleError::SymbolicLink(path.into()));
    }
    if metadata.is_file() {
        paths.insert(path.strip_prefix(root).expect("resource descendant").into());
    } else {
        for entry in fs::read_dir(path).map_err(|source| BundleError::Io {
            operation: "list resources",
            path: path.into(),
            source,
        })? {
            let entry = entry.map_err(|source| BundleError::Io {
                operation: "read resource entry",
                path: path.into(),
                source,
            })?;
            collect(root, &entry.path(), paths)?;
        }
    }
    Ok(())
}

fn read(path: &Path) -> Result<Vec<u8>, BundleError> {
    fs::read(path).map_err(|source| BundleError::Io {
        operation: "read resource",
        path: path.into(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sempre_state::{Layout, Store};
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};

    fn release(source: &Layout) {
        Store::new(source.clone()).initialize().unwrap();
        fs::create_dir_all(&source.resources).unwrap();
        fs::write(&source.service_executable, b"new executable").unwrap();
        crate::mark_release_directory(&source.root).unwrap();
    }

    #[test]
    fn unchanged_locked_driver_does_not_block_resource_upgrade() {
        let temporary = tempfile::tempdir().unwrap();
        let source = Layout::at(&temporary.path().join("source"));
        let target = Layout::system_at(&temporary.path().join("target"));
        release(&source);
        Store::new(target.clone()).initialize().unwrap();
        fs::create_dir_all(&target.resources).unwrap();
        fs::write(source.resources.join("driver.sys"), b"driver").unwrap();
        fs::write(target.resources.join("driver.sys"), b"driver").unwrap();
        fs::write(source.resources.join("helper.exe"), b"new helper").unwrap();
        fs::write(target.resources.join("helper.exe"), b"old helper").unwrap();
        fs::write(&target.service_executable, b"old executable").unwrap();
        let _lock = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(target.resources.join("driver.sys"))
            .unwrap();
        let mut transaction = crate::stage_install(&source, &target).unwrap();
        transaction.activate().unwrap();
        transaction.commit().unwrap();
        assert_eq!(
            fs::read(target.resources.join("helper.exe")).unwrap(),
            b"new helper"
        );
        assert_eq!(
            fs::read(&target.service_executable).unwrap(),
            b"new executable"
        );
    }

    #[test]
    fn changed_locked_resource_leaves_old_service_executable_in_place() {
        let temporary = tempfile::tempdir().unwrap();
        let source = Layout::at(&temporary.path().join("source"));
        let target = Layout::system_at(&temporary.path().join("target"));
        release(&source);
        Store::new(target.clone()).initialize().unwrap();
        fs::create_dir_all(&target.resources).unwrap();
        fs::write(source.resources.join("driver.sys"), b"new driver").unwrap();
        fs::write(target.resources.join("driver.sys"), b"old driver").unwrap();
        fs::write(&target.service_executable, b"old executable").unwrap();
        let _lock = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(target.resources.join("driver.sys"))
            .unwrap();
        let mut transaction = crate::stage_install(&source, &target).unwrap();
        assert!(transaction.activate().is_err());
        drop(transaction);
        assert_eq!(
            fs::read(&target.service_executable).unwrap(),
            b"old executable"
        );
        assert_eq!(
            fs::read(target.resources.join("driver.sys")).unwrap(),
            b"old driver"
        );
    }

    #[test]
    fn locked_executable_activation_preserves_old_program_and_rolls_back_resources() {
        let temporary = tempfile::tempdir().unwrap();
        let source = Layout::at(&temporary.path().join("source"));
        let target = Layout::system_at(&temporary.path().join("target"));
        release(&source);
        Store::new(target.clone()).initialize().unwrap();
        fs::create_dir_all(&target.resources).unwrap();
        fs::write(source.resources.join("helper.exe"), b"new helper").unwrap();
        fs::write(target.resources.join("helper.exe"), b"old helper").unwrap();
        fs::write(&target.service_executable, b"old executable").unwrap();
        let _lock = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&target.service_executable)
            .unwrap();
        let mut transaction = crate::stage_install(&source, &target).unwrap();
        assert!(transaction.activate().is_err());
        drop(transaction);
        assert_eq!(
            fs::read(&target.service_executable).unwrap(),
            b"old executable"
        );
        assert_eq!(
            fs::read(target.resources.join("helper.exe")).unwrap(),
            b"old helper"
        );
    }
}
