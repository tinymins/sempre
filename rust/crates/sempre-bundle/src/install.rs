use std::{fs, io::ErrorKind, path::Path};

use sempre_state::Layout;

use crate::BundleError;

use super::restore::{RestoreTransaction, Swap, validate_release};

pub fn stage_install(source: &Layout, target: &Layout) -> Result<RestoreTransaction, BundleError> {
    validate_release(&source.root)?;
    let mut transaction = RestoreTransaction::empty();
    for (from, to, required, executable) in [
        (
            &source.service_executable,
            &target.service_executable,
            true,
            true,
        ),
        (&source.resources, &target.resources, false, false),
        (&source.tools, &target.tools, false, false),
        (&source.ui, &target.ui, false, false),
    ] {
        transaction
            .operations
            .push(Swap::stage(from, to, required, executable)?);
    }
    for (from, to, required) in [
        (&source.cores, &target.cores, false),
        (&source.configs, &target.configs, false),
        (&source.subscriptions, &target.subscriptions, false),
        (&source.gateway, &target.gateway, false),
        (&source.tunnels, &target.tunnels, false),
        (&source.state, &target.state, true),
        (&source.web_config, &target.web_config, true),
    ] {
        if target_is_missing(to)? {
            transaction
                .operations
                .push(Swap::stage(from, to, required, false)?);
        }
    }
    Ok(transaction)
}

fn target_is_missing(path: &Path) -> Result<bool, BundleError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(source) if source.kind() == ErrorKind::NotFound => Ok(true),
        Err(source) => Err(BundleError::Io {
            operation: "inspect installation target",
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use sempre_state::Store;

    use super::*;
    use crate::mark_release_directory;

    #[test]
    fn upgrade_replaces_application_files_and_preserves_system_data() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = Layout::at(&temporary.path().join("source"));
        let target = Layout::system_at(&temporary.path().join("target"));
        prepare_release(&source);
        Store::new(target.clone())
            .initialize()
            .expect("target state");
        write(&target.service_executable, b"old executable");
        write(&target.resources.join("resource"), b"old resource");
        write(&target.tools.join("tool"), b"old tool");
        write(&target.ui.join("index.html"), b"old UI");
        write(&target.cores.join("existing/core.exe"), b"existing core");
        write(&target.configs.join("sing-box/config.json"), b"old config");
        write(&target.subscription_catalog, b"old subscriptions");
        write(&target.gateway.join("settings.json"), b"old gateway");
        write(&target.tunnels, b"old tunnels");
        write(&target.web_config, b"old web config");
        let state = fs::read(&target.state).expect("existing state");

        let mut transaction = stage_install(&source, &target).expect("stage upgrade");
        transaction.activate().expect("activate upgrade");
        transaction.commit().expect("commit upgrade");

        assert_eq!(
            fs::read(&target.service_executable).unwrap(),
            b"new executable"
        );
        assert_eq!(
            fs::read(target.resources.join("resource")).unwrap(),
            b"new resource"
        );
        assert_eq!(fs::read(target.tools.join("tool")).unwrap(), b"new tool");
        assert_eq!(fs::read(target.ui.join("index.html")).unwrap(), b"new UI");
        assert_eq!(fs::read(&target.state).unwrap(), state);
        assert_eq!(
            fs::read(&target.subscription_catalog).unwrap(),
            b"old subscriptions"
        );
        assert_eq!(
            fs::read(target.configs.join("sing-box/config.json")).unwrap(),
            b"old config"
        );
        assert_eq!(
            fs::read(target.cores.join("existing/core.exe")).unwrap(),
            b"existing core"
        );
        assert!(!target.cores.join("bundled/core.exe").exists());
        assert_eq!(
            fs::read(target.gateway.join("settings.json")).unwrap(),
            b"old gateway"
        );
        assert_eq!(fs::read(&target.tunnels).unwrap(), b"old tunnels");
        assert_eq!(fs::read(&target.web_config).unwrap(), b"old web config");
    }

    #[test]
    fn first_install_initializes_the_complete_release() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = Layout::at(&temporary.path().join("source"));
        let target = Layout::system_at(&temporary.path().join("target"));
        prepare_release(&source);

        let mut transaction = stage_install(&source, &target).expect("stage install");
        transaction.activate().expect("activate install");
        transaction.commit().expect("commit install");

        assert_eq!(
            fs::read(&target.state).unwrap(),
            fs::read(&source.state).unwrap()
        );
        assert_eq!(
            fs::read(target.cores.join("bundled/core.exe")).unwrap(),
            b"bundled core"
        );
        assert_eq!(
            fs::read(&target.subscription_catalog).unwrap(),
            b"new subscriptions"
        );
    }

    #[test]
    fn partial_install_preserves_existing_paths_and_initializes_missing_paths() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = Layout::at(&temporary.path().join("source"));
        let target = Layout::system_at(&temporary.path().join("target"));
        prepare_release(&source);
        write(&target.subscription_catalog, b"existing subscriptions");

        let mut transaction = stage_install(&source, &target).expect("stage partial install");
        transaction.activate().expect("activate partial install");
        transaction.commit().expect("commit partial install");

        assert_eq!(
            fs::read(&target.subscription_catalog).unwrap(),
            b"existing subscriptions"
        );
        assert_eq!(
            fs::read(&target.state).unwrap(),
            fs::read(&source.state).unwrap()
        );
    }

    fn prepare_release(layout: &Layout) {
        Store::new(layout.clone())
            .initialize()
            .expect("source state");
        write(&layout.service_executable, b"new executable");
        write(&layout.resources.join("resource"), b"new resource");
        write(&layout.tools.join("tool"), b"new tool");
        write(&layout.ui.join("index.html"), b"new UI");
        write(&layout.cores.join("bundled/core.exe"), b"bundled core");
        write(&layout.subscription_catalog, b"new subscriptions");
        write(
            &layout.web_config,
            br#"{"schema":1,"listen":"127.0.0.1:33211"}"#,
        );
        mark_release_directory(&layout.root).expect("release marker");
    }

    fn write(path: &std::path::Path, content: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory");
        }
        fs::write(path, content).expect("fixture");
    }
}
