use clap::Parser as _;

use super::*;

#[test]
fn administrator_boundary_matches_mutating_system_commands() {
    let version = Arguments::try_parse_from(["sempre", "version"]).expect("version");
    assert!(!version.requires_administrator(Mode::System));
    let portable_core = Arguments::try_parse_from(["sempre", "--portable", "core", "list"])
        .expect("portable core list");
    assert!(!portable_core.requires_administrator(Mode::Portable));
    let system_core = Arguments::try_parse_from(["sempre", "core", "list"]).expect("core list");
    assert!(system_core.requires_administrator(Mode::System));
    let portable_doctor =
        Arguments::try_parse_from(["sempre", "--portable", "doctor"]).expect("doctor");
    assert!(!portable_doctor.requires_administrator(Mode::Portable));
    let system_doctor = Arguments::try_parse_from(["sempre", "doctor"]).expect("doctor");
    assert!(system_doctor.requires_administrator(Mode::System));
    let development = Arguments::try_parse_from([
        "sempre",
        "daemon",
        "--development-root",
        ".cache/sempre-dev/runtime",
    ])
    .expect("development daemon");
    assert!(!development.requires_administrator(Mode::Development));
    let portable = Arguments::try_parse_from(["sempre", "--portable", "daemon"]).expect("daemon");
    assert!(portable.requires_administrator(Mode::Portable));

    let portable_run =
        Arguments::try_parse_from(["sempre", "portable", "run"]).expect("portable run");
    assert!(portable_run.requires_administrator(Mode::Portable));
    let portable_enable =
        Arguments::try_parse_from(["sempre", "portable", "enable"]).expect("portable enable");
    assert!(!portable_enable.requires_administrator(Mode::System));

    let deploy =
        Arguments::try_parse_from(["sempre", "--portable", "service", "deploy", "data", "--yes"])
            .expect("service data deploy");
    assert!(deploy.requires_administrator(Mode::Portable));
    assert!(matches!(
        deploy.command,
        Command::Service {
            command: ServiceCommand::Deploy {
                component: ServiceDeployComponent::Data,
                yes: true,
            }
        }
    ));
}
