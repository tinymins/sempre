use std::{
    fs, io,
    path::{Path, PathBuf},
};

use sempre_state::Layout;
use serde::Deserialize;
use uuid::Uuid;

use crate::{BundleError, METADATA_NAME, PORTABLE_MARKER, copy_file, copy_tree};

#[derive(Deserialize)]
struct SnapshotMetadata {
    schema: u32,
    kind: String,
}

pub struct RestoreTransaction {
    operations: Vec<Swap>,
    committed: bool,
}

struct Swap {
    target: PathBuf,
    staged: Option<PathBuf>,
    backup: PathBuf,
    had_target: bool,
    activated: bool,
}

pub fn validate_snapshot(root: &Path) -> Result<(), BundleError> {
    let metadata_path = root.join(METADATA_NAME);
    let data = fs::read(&metadata_path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            BundleError::Missing(metadata_path.clone())
        } else {
            BundleError::Io {
                operation: "read snapshot metadata",
                path: metadata_path.clone(),
                source: error,
            }
        }
    })?;
    let metadata: SnapshotMetadata =
        serde_json::from_slice(&data).map_err(|source| BundleError::Decode {
            name: "snapshot metadata",
            source,
        })?;
    if metadata.schema != 1 || metadata.kind != "snapshot" {
        return Err(BundleError::InvalidMetadata(format!(
            "expected schema 1 snapshot, found schema {} {}",
            metadata.schema, metadata.kind
        )));
    }
    let marker = root.join(PORTABLE_MARKER);
    if !marker.is_file() {
        return Err(BundleError::Missing(marker));
    }
    Ok(())
}

pub fn stage_restore(source: &Layout, target: &Layout) -> Result<RestoreTransaction, BundleError> {
    validate_snapshot(&source.root)?;
    let mut transaction = RestoreTransaction {
        operations: Vec::new(),
        committed: false,
    };
    for (from, to, required, executable) in [
        (
            &source.service_executable,
            &target.service_executable,
            true,
            true,
        ),
        (&source.resources, &target.resources, false, false),
        (&source.tools, &target.tools, false, false),
        (&source.cores, &target.cores, false, false),
        (&source.configs, &target.configs, false, false),
        (&source.subscriptions, &target.subscriptions, false, false),
        (&source.gateway, &target.gateway, false, false),
        (&source.ui, &target.ui, false, false),
        (&source.tunnels, &target.tunnels, false, false),
        (&source.state, &target.state, true, false),
        (&source.web_config, &target.web_config, true, false),
    ] {
        transaction
            .operations
            .push(Swap::stage(from, to, required, executable)?);
    }
    Ok(transaction)
}

impl RestoreTransaction {
    pub fn activate(&mut self) -> Result<(), BundleError> {
        for operation in &mut self.operations {
            if let Err(error) = operation.activate() {
                self.rollback();
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn rollback(&mut self) {
        for operation in self.operations.iter_mut().rev() {
            operation.rollback();
        }
    }

    pub fn commit(mut self) -> Result<(), BundleError> {
        self.committed = true;
        let mut first_error = None;
        for operation in &mut self.operations {
            if let Err(error) = operation.cleanup() {
                first_error.get_or_insert(error);
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Drop for RestoreTransaction {
    fn drop(&mut self) {
        if !self.committed {
            self.rollback();
        }
    }
}

impl Swap {
    fn stage(
        source: &Path,
        target: &Path,
        required: bool,
        executable: bool,
    ) -> Result<Self, BundleError> {
        let metadata = match source.symlink_metadata() {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == io::ErrorKind::NotFound && !required => None,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(BundleError::Missing(source.to_path_buf()));
            }
            Err(source_error) => {
                return Err(BundleError::Io {
                    operation: "inspect snapshot source",
                    path: source.to_path_buf(),
                    source: source_error,
                });
            }
        };
        if metadata
            .as_ref()
            .is_some_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(BundleError::SymbolicLink(source.to_path_buf()));
        }
        let parent = target.parent().ok_or_else(|| {
            BundleError::InvalidMetadata(format!("target has no parent: {}", target.display()))
        })?;
        fs::create_dir_all(parent).map_err(|source_error| BundleError::Io {
            operation: "create deployment parent",
            path: parent.to_path_buf(),
            source: source_error,
        })?;
        let staged = metadata
            .map(|metadata| {
                let staged = unique_sibling(target, "stage");
                let result = if metadata.is_dir() {
                    copy_tree(source, &staged)
                } else if metadata.is_file() {
                    copy_file(source, &staged, executable)
                } else {
                    Err(BundleError::InvalidMetadata(format!(
                        "unsupported snapshot entry: {}",
                        source.display()
                    )))
                };
                if let Err(error) = result {
                    let _ = remove_path(&staged);
                    return Err(error);
                }
                Ok(staged)
            })
            .transpose()?;
        Ok(Self {
            target: target.to_path_buf(),
            staged,
            backup: unique_sibling(target, "backup"),
            had_target: false,
            activated: false,
        })
    }

    fn activate(&mut self) -> Result<(), BundleError> {
        self.had_target = match self.target.symlink_metadata() {
            Ok(_) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(source) => {
                return Err(BundleError::Io {
                    operation: "inspect deployment target",
                    path: self.target.clone(),
                    source,
                });
            }
        };
        if self.had_target {
            rename(&self.target, &self.backup, "back up deployment target")?;
        }
        if let Some(staged) = &self.staged
            && let Err(error) = rename(staged, &self.target, "activate deployment target")
        {
            if self.had_target {
                let _ = fs::rename(&self.backup, &self.target);
            }
            return Err(error);
        }
        self.activated = true;
        Ok(())
    }

    fn rollback(&mut self) {
        if self.activated {
            let _ = remove_path(&self.target);
            if self.had_target {
                let _ = fs::rename(&self.backup, &self.target);
            }
            self.activated = false;
        }
        if let Some(staged) = &self.staged {
            let _ = remove_path(staged);
        }
        if !self.had_target {
            let _ = remove_path(&self.backup);
        }
    }

    fn cleanup(&mut self) -> Result<(), BundleError> {
        if let Some(staged) = &self.staged {
            remove_path(staged)?;
        }
        remove_path(&self.backup)
    }
}

fn unique_sibling(target: &Path, kind: &str) -> PathBuf {
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    target.with_file_name(format!(".{name}.sempre-{kind}-{}", Uuid::new_v4()))
}

fn rename(source: &Path, target: &Path, operation: &'static str) -> Result<(), BundleError> {
    fs::rename(source, target).map_err(|source_error| BundleError::Io {
        operation,
        path: source.to_path_buf(),
        source: source_error,
    })
}

fn remove_path(path: &Path) -> Result<(), BundleError> {
    let metadata = match path.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(BundleError::Io {
                operation: "inspect staged deployment path",
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let result = if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    result.map_err(|source| BundleError::Io {
        operation: "remove staged deployment path",
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sempre_state::Store;

    #[test]
    fn restore_transaction_rolls_back_and_commits_complete_snapshots() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = Layout::at(&temporary.path().join("source"));
        let target = Layout::system_at(&temporary.path().join("target"));
        Store::new(source.clone())
            .initialize()
            .expect("source state");
        fs::write(&source.service_executable, b"new executable").expect("source executable");
        fs::write(
            &source.web_config,
            br#"{"schema":1,"listen":"127.0.0.1:33211"}"#,
        )
        .expect("source web config");
        fs::write(
            source.root.join(METADATA_NAME),
            br#"{"schema":1,"kind":"snapshot"}"#,
        )
        .expect("snapshot metadata");
        fs::write(source.root.join(PORTABLE_MARKER), b"").expect("portable marker");
        fs::write(&source.subscription_catalog, b"new catalog").expect("source catalog");

        if let Some(parent) = target.service_executable.parent() {
            fs::create_dir_all(parent).expect("target binary directory");
        }
        if let Some(parent) = target.state.parent() {
            fs::create_dir_all(parent).expect("target state directory");
        }
        fs::write(&target.service_executable, b"old executable").expect("target executable");
        fs::write(&target.state, b"old state").expect("target state");

        let mut rollback = stage_restore(&source, &target).expect("stage rollback");
        rollback.activate().expect("activate rollback");
        assert_eq!(
            fs::read(&target.service_executable).expect("active executable"),
            b"new executable"
        );
        rollback.rollback();
        assert_eq!(
            fs::read(&target.service_executable).expect("rolled back executable"),
            b"old executable"
        );
        assert_eq!(
            fs::read(&target.state).expect("rolled back state"),
            b"old state"
        );

        let mut commit = stage_restore(&source, &target).expect("stage commit");
        commit.activate().expect("activate commit");
        commit.commit().expect("commit restore");
        assert_eq!(
            fs::read(&target.subscription_catalog).expect("installed catalog"),
            b"new catalog"
        );
        assert!(
            walk(&temporary)
                .iter()
                .all(|path| !path.to_string_lossy().contains(".sempre-stage-")
                    && !path.to_string_lossy().contains(".sempre-backup-"))
        );
    }

    fn walk(root: &tempfile::TempDir) -> Vec<PathBuf> {
        fn collect(path: &Path, output: &mut Vec<PathBuf>) {
            let Ok(entries) = fs::read_dir(path) else {
                return;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                output.push(path.clone());
                if path.is_dir() {
                    collect(&path, output);
                }
            }
        }
        let mut paths = Vec::new();
        collect(root.path(), &mut paths);
        paths
    }
}
