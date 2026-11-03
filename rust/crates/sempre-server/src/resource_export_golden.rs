//! Byte-level regression guard over the server export path.
//!
//! `SEMPRE_GOLDEN_UPDATE=1` rewrites the manifest after a reviewed output change, and
//! `SEMPRE_GOLDEN_DUMP=<dir>` writes the rendered outputs so two revisions can be diffed.
use std::{env, fmt::Write as _, fs, path::Path};

use sempre_converter::{CompileRequest, Target};
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;

const REQUEST: &str = include_str!("../../sempre-converter/tests/fixtures/golden_request.json");
const MANIFEST: &str = "src/resource_export_golden.sha256";
const TARGETS: [&str; 15] = [
    "clash",
    "clash-meta",
    "clash-rs",
    "dae",
    "xray",
    "v2ray",
    "sing-box",
    "sing-box-v12",
    "sing-box-v13",
    "sing-box-v14",
    "sing-box-v13-windows",
    "sing-box-v13-macos",
    "sing-box-v12-openwrt",
    "sing-box-v13-openwrt",
    "sing-box-v14-openwrt",
];

fn request(format: &str) -> CompileRequest {
    let mut document: Value = serde_json::from_str(REQUEST).expect("fixture");
    if matches!(format, "xray" | "v2ray") {
        let profile = document["profile"].as_object_mut().expect("profile");
        for field in ["groups", "rules", "rule_providers"] {
            profile.remove(field);
        }
    }
    let mut target = Target::parse(format).expect("target");
    target.standalone = true;
    document["protocol"] = 1.into();
    document["target"] = serde_json::to_value(target).expect("target");
    serde_json::from_value(document).expect("request")
}

#[test]
fn exported_outputs_match_the_reviewed_manifest() {
    let base = Url::parse("https://sempre.example.test/").expect("base");
    let mut manifest = String::new();
    for format in TARGETS {
        let result = crate::resource_export::compile(&request(format), &base)
            .unwrap_or_else(|_| panic!("{format} must export"));
        if let Some(directory) = env::var_os("SEMPRE_GOLDEN_DUMP") {
            let directory = Path::new(&directory).join("server");
            fs::create_dir_all(&directory).expect("dump directory");
            fs::write(directory.join(format), &result.content).expect("dump output");
        }
        let digest = Sha256::digest(result.content.as_bytes());
        writeln!(manifest, "{digest:x}  {format}").expect("write manifest");
    }
    if env::var_os("SEMPRE_GOLDEN_UPDATE").is_some() {
        fs::write(MANIFEST, &manifest).expect("update manifest");
        return;
    }
    let expected = fs::read_to_string(MANIFEST).expect("manifest");
    assert_eq!(manifest, expected, "exported output changed");
}
