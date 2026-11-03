//! Byte-level regression guard over every compile target.
//!
//! `SEMPRE_GOLDEN_UPDATE=1` rewrites the manifest after a reviewed output change, and
//! `SEMPRE_GOLDEN_DUMP=<dir>` writes the rendered outputs so two revisions can be diffed.
use std::{collections::BTreeMap, env, fmt::Write as _, fs, path::Path};

use sempre_converter::{CompileRequest, Target, available_targets, compile};
use serde_json::Value;
use sha2::{Digest, Sha256};

const REQUEST: &str = include_str!("fixtures/golden_request.json");
const MANIFEST: &str = "tests/fixtures/golden_converter.sha256";
const OPENWRT_TARGETS: [&str; 4] = [
    "sing-box-openwrt",
    "sing-box-v12-openwrt",
    "sing-box-v13-openwrt",
    "sing-box-v14-openwrt",
];

fn request(target: Target) -> CompileRequest {
    let mut document: Value = serde_json::from_str(REQUEST).expect("fixture");
    document["protocol"] = 1.into();
    // Node-only engines have no selector model, so they render the node set alone.
    if matches!(target.format.as_str(), "xray" | "v2ray") {
        let profile = document["profile"].as_object_mut().expect("profile");
        for field in ["groups", "rules", "rule_providers"] {
            profile.remove(field);
        }
    }
    document["target"] = serde_json::to_value(target).expect("target");
    serde_json::from_value(document).expect("request")
}

fn rendered_outputs() -> BTreeMap<String, String> {
    let mut targets = available_targets()
        .into_iter()
        .map(|target| (target.format.clone(), target))
        .collect::<Vec<_>>();
    for format in OPENWRT_TARGETS {
        targets.push((format.into(), Target::parse(format).expect("target")));
    }
    let mut outputs = BTreeMap::new();
    for (name, target) in targets {
        let mut standalone = target.clone();
        standalone.standalone = true;
        for (name, target) in [
            (name.clone(), target),
            (format!("{name}.standalone"), standalone),
        ] {
            let content = compile(&request(target))
                .map_or_else(|error| format!("error: {error}"), |result| result.content);
            outputs.insert(name, content);
        }
    }
    outputs
}

#[test]
fn compiled_outputs_match_the_reviewed_manifest() {
    let outputs = rendered_outputs();
    if let Some(directory) = env::var_os("SEMPRE_GOLDEN_DUMP") {
        let directory = Path::new(&directory).join("converter");
        fs::create_dir_all(&directory).expect("dump directory");
        for (name, content) in &outputs {
            fs::write(directory.join(name), content).expect("dump output");
        }
    }
    let manifest = outputs
        .iter()
        .fold(String::new(), |mut manifest, (name, content)| {
            let digest = Sha256::digest(content.as_bytes());
            writeln!(manifest, "{digest:x}  {name}").expect("write manifest");
            manifest
        });
    if env::var_os("SEMPRE_GOLDEN_UPDATE").is_some() {
        fs::write(MANIFEST, &manifest).expect("update manifest");
        return;
    }
    let expected = fs::read_to_string(MANIFEST).expect("manifest");
    assert_eq!(manifest, expected, "compiled output changed");
}

#[test]
fn golden_fixture_renders_every_supported_target() {
    let errors = rendered_outputs()
        .into_iter()
        .filter(|(_, content)| content.starts_with("error: "))
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{errors:?}");
}
