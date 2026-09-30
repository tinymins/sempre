//! Public export paths normalize to one versioned consumer target.
use sempre_converter::{CompileError, Target};

pub(crate) fn parse(format: &str) -> Result<Target, CompileError> {
    let mut target = Target::parse(format)?;
    if target.core == "sing-box" && target.platform == "default" {
        target = Target::parse(&format!("{format}-openwrt"))?;
    }
    target.standalone = true;
    Ok(target)
}

pub(crate) fn available() -> Vec<Target> {
    sempre_converter::available_targets()
        .into_iter()
        .map(|target| parse(&target.format).expect("supported export target"))
        .collect()
}

pub(crate) fn from_path(suffix: &str) -> Option<Target> {
    let parts = suffix.split('/').collect::<Vec<_>>();
    let (version, consumer) = match parts.as_slice() {
        [format] => return parse(format).ok(),
        ["sing-box", consumer @ ("windows" | "macos")] => ("11", *consumer),
        ["sing-box", version @ ("12" | "13" | "14")] => (*version, "openwrt"),
        [
            "sing-box",
            version,
            consumer @ ("openwrt" | "windows" | "macos"),
        ] => {
            let version = match *version {
                "1.11" => "11",
                "1.12" | "12" => "12",
                "1.13" | "13" => "13",
                "1.14" | "14" => "14",
                _ => return None,
            };
            (version, *consumer)
        }
        _ => return None,
    };
    let version = if version == "11" {
        String::new()
    } else {
        format!("-v{version}")
    };
    parse(&format!("sing-box{version}-{consumer}")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_paths_and_legacy_aliases_share_one_target() {
        for version in ["11", "12", "13", "14"] {
            for consumer in ["openwrt", "windows", "macos"] {
                let target = from_path(&format!("sing-box/1.{version}/{consumer}")).unwrap();
                assert_eq!(target.version, version);
                assert_eq!(target.platform, consumer);
                assert!(target.standalone);
                let legacy = match (version, consumer) {
                    ("11", "openwrt") => "sing-box".into(),
                    ("11", _) => format!("sing-box/{consumer}"),
                    (_, "openwrt") => format!("sing-box/{version}"),
                    _ => format!("sing-box/{version}/{consumer}"),
                };
                assert_eq!(from_path(&legacy).unwrap().format, target.format);
            }
        }
    }

    #[test]
    fn malformed_or_unsupported_paths_do_not_fall_back() {
        for path in [
            "sing-box/1.15/openwrt",
            "sing-box/1.13/linux",
            "sing-box/1.13",
            "sing-box//13",
            "sing-box/13/",
            "sing-box/1.13/openwrt/extra",
        ] {
            assert!(from_path(path).is_none(), "{path}");
        }
        assert_eq!(from_path("clash").unwrap().format, "clash");
    }
}
