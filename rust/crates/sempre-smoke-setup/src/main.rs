use std::{fs, path::PathBuf};

use chrono::Utc;
use clap::Parser;
use sempre_state::{Deployment, Installation, Layout, Selection, Store, write_atomic};
use sha2::{Digest as _, Sha256};

#[derive(Parser)]
struct Arguments {
    #[arg(long)]
    root: PathBuf,
    #[arg(long)]
    core: PathBuf,
}

fn main() {
    let arguments = Arguments::parse();
    if let Err(error) = setup(&arguments.root, &arguments.core) {
        eprintln!("ERROR: prepare smoke release: {error}");
        std::process::exit(1);
    }
}

fn setup(root: &std::path::Path, core: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    if !core.is_file() {
        return Err(format!("test core is unavailable: {}", core.display()).into());
    }
    let layout = Layout::at(root);
    let store = Store::new(layout.clone());
    store.initialize()?;
    sempre_control::WebConfigStore::new(&layout.web_config).initialize()?;
    let subscriptions = sempre_subscription::SubscriptionStore::new(layout.clone());
    subscriptions.initialize()?;
    subscriptions.update(|catalog| {
        catalog.profiles[0].transparent_proxy.mode = "disabled".into();
        Ok(())
    })?;

    let core_data = fs::read(core)?;
    let core_digest = format!("sha256:{:x}", Sha256::digest(&core_data));
    write_atomic(
        &layout.core_binary("sing-box", None, "1.2.3"),
        &core_data,
        0o755,
    )?;
    let config_data = br#"{"log":{"disabled":true},"inbounds":[],"outbounds":[]}"#;
    let config_hash = format!("{:x}", Sha256::digest(config_data));
    write_atomic(&layout.config("sing-box", &config_hash), config_data, 0o600)?;
    store.update(|document| {
        let source = &mut document.core_mut("sing-box").default;
        source.channels.insert("stable".into(), "1.2.3".into());
        source.installed.insert(
            "1.2.3".into(),
            Installation {
                explicit: true,
                digest: core_digest,
                source: "integration-test".into(),
                installed_at: Utc::now(),
            },
        );
        document.selected = Some(Selection {
            core: "sing-box".into(),
            repository: None,
            reference: "stable".into(),
        });
        document
            .configs
            .insert("sing-box".into(), config_hash.clone());
        document.active = Some(Deployment {
            core: "sing-box".into(),
            repository: None,
            reference: "stable".into(),
            version: "1.2.3".into(),
            config_hash,
        });
        document.subscription.interval = "off".into();
        Ok(())
    })?;
    sempre_bundle::mark_release_directory(&layout.root)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_is_a_valid_release_with_a_runnable_core_selection() {
        let root = tempfile::tempdir().expect("release root");
        let core_root = tempfile::tempdir().expect("core root");
        let core = core_root.path().join(if cfg!(windows) {
            "testcore.exe"
        } else {
            "testcore"
        });
        fs::write(&core, b"test core").expect("test core");
        setup(root.path(), &core).expect("setup");
        sempre_bundle::validate_release(root.path()).expect("release marker");
        let document = Store::new(Layout::at(root.path())).read().expect("state");
        assert_eq!(document.selected.expect("selection").reference, "stable");
        assert!(document.active.is_some());
        let catalog = sempre_subscription::SubscriptionStore::new(Layout::at(root.path()))
            .read()
            .expect("subscriptions");
        assert_eq!(catalog.profiles[0].transparent_proxy.mode, "disabled");
    }
}
