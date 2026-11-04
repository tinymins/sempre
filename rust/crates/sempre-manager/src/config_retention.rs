//! Compiled configurations are stored by hash, so each change adds a file. Only the
//! current one and the previous version are kept.
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use sempre_state::{Document, Store};

use crate::ManagerError;

const HASH_LENGTH: usize = 64;

/// Skips the cleanup when another operation is staging a configuration not yet recorded.
pub(crate) fn prune_superseded_configs(store: &Store) -> Result<(), ManagerError> {
    let Some(_operation) = store.try_acquire_operation()? else {
        return Ok(());
    };
    let Some(_config) = store.try_acquire_config()? else {
        return Ok(());
    };
    let document = store.read()?;
    let directory = &store.layout().configs;
    let Ok(cores) = fs::read_dir(directory) else {
        return Ok(());
    };
    for core in cores.flatten() {
        let name = core.file_name();
        let kept = retained_hashes(&document, &name.to_string_lossy());
        prune_core(&core.path(), &kept)
            .map_err(|error| ManagerError::io("remove superseded configuration", error))?;
    }
    Ok(())
}

fn retained_hashes<'a>(document: &'a Document, core: &str) -> HashSet<&'a str> {
    [
        &document.active,
        &document.last_good,
        &document.rejected_deployment,
    ]
    .into_iter()
    .flatten()
    .filter(|deployment| deployment.core == core)
    .map(|deployment| deployment.config_hash.as_str())
    .chain(document.configs.get(core).map(String::as_str))
    .collect()
}

fn prune_core(directory: &Path, kept: &HashSet<&str>) -> std::io::Result<()> {
    let mut stale: HashMap<String, (SystemTime, Vec<PathBuf>)> = HashMap::new();
    for entry in fs::read_dir(directory)?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // A hash names the configuration and its sidecars; anything else is not ours.
        let hash = name.split('.').next().unwrap_or_default();
        if hash.len() == HASH_LENGTH
            && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !kept.contains(hash)
        {
            let modified = entry.metadata()?.modified()?;
            let group = stale
                .entry(hash.to_owned())
                .or_insert((modified, Vec::new()));
            group.0 = group.0.max(modified);
            group.1.push(entry.path());
        }
    }
    // A proven deployment is its own rollback target, so keep the version it replaced.
    let previous = (kept.len() < 2)
        .then(|| {
            stale
                .iter()
                .max_by_key(|(_, (modified, _))| *modified)
                .map(|(hash, _)| hash.clone())
        })
        .flatten();
    for (hash, (_, files)) in stale {
        if previous.as_ref() != Some(&hash) {
            for file in files {
                fs::remove_file(file)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sempre_state::{Deployment, Layout};

    use super::*;

    fn deployment(hash: char) -> Deployment {
        Deployment {
            core: "sing-box".into(),
            repository: None,
            reference: "stable".into(),
            version: "1.13.21".into(),
            config_hash: hash.to_string().repeat(HASH_LENGTH),
        }
    }

    #[test]
    fn a_pending_deployment_keeps_the_current_and_rollback_configurations_only() {
        let directory = tempfile::tempdir().expect("directory");
        let document = Document {
            active: Some(deployment('a')),
            last_good: Some(deployment('b')),
            ..Document::default()
        };
        let configs = directory.path().join("sing-box");
        fs::create_dir_all(&configs).expect("configs");
        let file = |name: String| configs.join(name);
        let names = |hash: char| {
            let hash = hash.to_string().repeat(HASH_LENGTH);
            [
                format!("{hash}.json"),
                format!("{hash}.dns-rule-snapshot.json"),
            ]
        };
        for hash in ['a', 'b', 'c'] {
            for name in names(hash) {
                fs::write(file(name), b"{}").expect("config");
            }
        }
        fs::write(file("notes.txt".into()), b"keep").expect("other file");

        prune_core(&configs, &retained_hashes(&document, "sing-box")).expect("prune");

        for name in names('a').into_iter().chain(names('b')) {
            assert!(file(name).exists());
        }
        for name in names('c') {
            assert!(!file(name).exists());
        }
        assert!(file("notes.txt".into()).exists());
    }

    #[test]
    fn a_proven_deployment_keeps_the_most_recent_previous_version() {
        let directory = tempfile::tempdir().expect("directory");
        let path = |hash: char| {
            directory
                .path()
                .join(format!("{}.json", hash.to_string().repeat(HASH_LENGTH)))
        };
        for (hash, age) in [('a', 0), ('b', 60), ('c', 120)] {
            let file = fs::File::create(path(hash)).expect("config");
            file.set_modified(SystemTime::now() - std::time::Duration::from_secs(age))
                .expect("time");
        }
        let current = "a".repeat(HASH_LENGTH);
        prune_core(directory.path(), &HashSet::from([current.as_str()])).expect("prune");
        assert!(path('a').exists());
        assert!(path('b').exists());
        assert!(!path('c').exists());
    }

    #[test]
    fn a_running_operation_postpones_the_cleanup() {
        let directory = tempfile::tempdir().expect("directory");
        let store = Store::new(Layout::at(directory.path()));
        store.initialize().expect("state");
        let configs = store.layout().configs.join("sing-box");
        fs::create_dir_all(&configs).expect("configs");
        let staged = configs.join(format!("{}.json", "c".repeat(HASH_LENGTH)));
        fs::write(&staged, b"{}").expect("config");
        let operation = store.acquire_operation().expect("operation");
        prune_superseded_configs(&store).expect("prune");
        assert!(staged.exists());
        drop(operation);
    }
}
