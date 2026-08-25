use clap::Parser as _;

use super::*;

#[test]
fn system_and_portable_modes_are_mutually_exclusive() {
    assert!(Arguments::try_parse_from(["sempre", "--system", "--portable", "version"]).is_err());
    let status = Arguments::try_parse_from(["sempre", "service", "status"]).expect("status");
    assert!(!status.requires_administrator());
    let restart = Arguments::try_parse_from(["sempre", "service", "restart"]).expect("restart");
    assert!(restart.requires_administrator());
    let runtime =
        Arguments::try_parse_from(["sempre", "--portable", "--json", "runtime", "status"])
            .expect("portable runtime status");
    assert!(!runtime.requires_administrator());
    assert!(runtime.json);
}

#[test]
fn parses_core_install_and_daemon_override() {
    let install =
        Arguments::try_parse_from(["sempre", "--portable", "core", "install", "sing-box@1.13.0"])
            .expect("core install");
    assert!(matches!(
        install.command,
        Command::Core {
            command: CoreCommand::Install { .. }
        }
    ));
    let daemon = Arguments::try_parse_from([
        "sempre",
        "daemon",
        "--listen",
        "127.0.0.1:44000",
        "--development-root",
        ".cache/sempre-dev/runtime",
    ])
    .expect("daemon");
    assert!(matches!(
        daemon.command,
        Command::Daemon {
            listen: Some(_),
            development_root: Some(_)
        }
    ));
    let select =
        Arguments::try_parse_from(["sempre", "core", "use", "sing-box@stable"]).expect("use");
    assert!(matches!(
        select.command,
        Command::Core {
            command: CoreCommand::Use { .. }
        }
    ));
    let restore = Arguments::try_parse_from(["sempre", "bundle", "restore", "--yes"])
        .expect("bundle restore");
    assert!(matches!(
        restore.command,
        Command::Bundle {
            command: BundleCommand::Restore { yes: true }
        }
    ));
    let install = Arguments::try_parse_from(["sempre", "install", "--yes"]).expect("install");
    assert!(install.requires_administrator());
    assert!(matches!(
        install.command,
        Command::Install { yes: true, .. }
    ));
    let uninstall =
        Arguments::try_parse_from(["sempre", "uninstall", "--purge", "--yes"]).expect("uninstall");
    assert!(uninstall.requires_administrator());
    assert!(matches!(
        uninstall.command,
        Command::Uninstall {
            purge: true,
            yes: true
        }
    ));
    let configured = Arguments::try_parse_from([
        "sempre",
        "install",
        "--core=mihomo@stable",
        "--subscription-file=subscription.txt",
        "--ui=https://example.com/ui.zip",
        "--ui-sha256",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ])
    .expect("configured install");
    assert!(matches!(
        configured.command,
        Command::Install {
            core: Some(_),
            subscription_file: Some(_),
            ui: Some(_),
            ui_sha256: Some(_),
            ..
        }
    ));
    assert!(
        Arguments::try_parse_from([
            "sempre",
            "install",
            "--subscription=https://example.com/sub",
            "--subscription-file=subscription.txt",
        ])
        .is_err()
    );
}

#[test]
fn parses_core_update_and_config_import() {
    let update = Arguments::try_parse_from(["sempre", "core", "update"]).expect("all channels");
    assert!(matches!(
        update.command,
        Command::Core {
            command: CoreCommand::Update { reference: None }
        }
    ));
    let update = Arguments::try_parse_from(["sempre", "update"]).expect("global update");
    assert!(matches!(update.command, Command::Update));
    let import = Arguments::try_parse_from([
        "sempre",
        "--portable",
        "config",
        "import",
        "subscription.yaml",
    ])
    .expect("config import");
    assert!(matches!(
        import.command,
        Command::Config {
            command: ConfigCommand::Import { .. }
        }
    ));
    let custom = Arguments::try_parse_from([
        "sempre",
        "--portable",
        "custom-node",
        "update",
        "node-id",
        "node.json",
    ])
    .expect("custom node update");
    assert!(matches!(
        custom.command,
        Command::CustomNode {
            command: CustomNodeCommand::Update { .. }
        }
    ));
}

#[test]
fn administrator_boundary_matches_mutating_system_commands() {
    let version = Arguments::try_parse_from(["sempre", "version"]).expect("version");
    assert!(!version.requires_administrator());
    let portable_core = Arguments::try_parse_from(["sempre", "--portable", "core", "list"])
        .expect("portable core list");
    assert!(!portable_core.requires_administrator());
    let system_core = Arguments::try_parse_from(["sempre", "core", "list"]).expect("core list");
    assert!(system_core.requires_administrator());
    let portable_doctor =
        Arguments::try_parse_from(["sempre", "--portable", "doctor"]).expect("doctor");
    assert!(!portable_doctor.requires_administrator());
    let system_doctor = Arguments::try_parse_from(["sempre", "doctor"]).expect("doctor");
    assert!(system_doctor.requires_administrator());
    let development = Arguments::try_parse_from([
        "sempre",
        "daemon",
        "--development-root",
        ".cache/sempre-dev/runtime",
    ])
    .expect("development daemon");
    assert!(!development.requires_administrator());
    let portable = Arguments::try_parse_from(["sempre", "--portable", "daemon"]).expect("daemon");
    assert!(portable.requires_administrator());
}
