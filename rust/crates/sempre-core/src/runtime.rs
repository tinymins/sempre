use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
};

use rand::random;
use serde_json::{Map, Value, json};

use crate::{BuiltInKind, ControlProtocol, ControlSpec, RegistryError, RuntimeSpec};

const SAFE_ORIGIN: &str = "http://localhost.invalid";

pub(super) fn prepare(
    kind: BuiltInKind,
    config: &Path,
    runtime_directory: &Path,
) -> Result<RuntimeSpec, RegistryError> {
    match kind {
        BuiltInKind::SingBox => prepare_sing_box(config, runtime_directory),
        BuiltInKind::Mihomo | BuiltInKind::ClashRs => {
            prepare_clash_yaml(kind, config, runtime_directory)
        }
        BuiltInKind::Xray | BuiltInKind::V2Ray => {
            prepare_v2ray_family(kind, config, runtime_directory)
        }
        BuiltInKind::Dae => Ok(RuntimeSpec {
            config: config.to_path_buf(),
            control: None,
        }),
    }
}

fn prepare_sing_box(config: &Path, directory: &Path) -> Result<RuntimeSpec, RegistryError> {
    let core = "sing-box";
    let mut document = read_json_object(core, config)?;
    let control = private_control(core, ControlProtocol::ClashRest)?;
    let experimental = object_mut(document.entry("experimental").or_insert(Value::Null));
    let clash_api = object_mut(experimental.entry("clash_api").or_insert(Value::Null));
    clash_api.insert(
        "external_controller".into(),
        json!(control_address(&control)),
    );
    clash_api.insert("secret".into(), json!(control.secret));
    clash_api.insert("external_ui".into(), json!(""));
    clash_api.insert("external_ui_download_url".into(), json!(""));
    clash_api.insert("external_ui_download_detour".into(), json!(""));
    clash_api.insert("access_control_allow_origin".into(), json!([SAFE_ORIGIN]));
    clash_api.insert("access_control_allow_private_network".into(), json!(false));

    let encoded = serde_json::to_vec_pretty(&document)
        .map_err(|error| runtime_error(core, format!("encode configuration: {error}")))?;
    let path = write_runtime(core, directory, "config.json", &encoded, true)?;
    Ok(RuntimeSpec {
        config: path,
        control: Some(control),
    })
}

fn prepare_clash_yaml(
    kind: BuiltInKind,
    config: &Path,
    directory: &Path,
) -> Result<RuntimeSpec, RegistryError> {
    let core = core_id(kind);
    let data = fs::read(config)
        .map_err(|error| runtime_error(core, format!("read configuration: {error}")))?;
    let mut document: Value = serde_yaml::from_slice(&data)
        .map_err(|error| runtime_error(core, format!("decode configuration: {error}")))?;
    let document = document
        .as_object_mut()
        .ok_or_else(|| runtime_error(core, "configuration root must be an object"))?;
    let control = private_control(core, ControlProtocol::ClashRest)?;

    for key in [
        "external-controller-tls",
        "external-controller-unix",
        "external-controller-pipe",
        "external-doh-server",
        "external-ui",
        "external-ui-name",
        "external-ui-url",
        "external-ui-headers",
    ] {
        document.remove(key);
    }
    document.insert(
        "external-controller".into(),
        json!(control_address(&control)),
    );
    document.insert("secret".into(), json!(control.secret));
    if kind == BuiltInKind::ClashRs {
        document.remove("external-controller-cors");
        document.insert("cors-allow-origins".into(), json!([SAFE_ORIGIN]));
    } else {
        document.insert(
            "external-controller-cors".into(),
            json!({
                "allow-origins": [SAFE_ORIGIN],
                "allow-private-network": false,
            }),
        );
    }

    let encoded = serde_yaml::to_string(&document)
        .map_err(|error| runtime_error(core, format!("encode configuration: {error}")))?;
    let path = write_runtime(core, directory, "config.yaml", encoded.as_bytes(), false)?;
    Ok(RuntimeSpec {
        config: path,
        control: Some(control),
    })
}

fn prepare_v2ray_family(
    kind: BuiltInKind,
    config: &Path,
    directory: &Path,
) -> Result<RuntimeSpec, RegistryError> {
    let core = core_id(kind);
    let mut document = read_json_object(core, config)?;
    let control = private_control(core, ControlProtocol::Grpc)?;
    let port = control
        .base_url
        .strip_prefix("http://")
        .and_then(|address| address.rsplit_once(':'))
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .ok_or_else(|| runtime_error(core, "reserved control address has no valid port"))?;

    document.insert(
        "api".into(),
        json!({
            "tag": "sempre-api",
            "services": [
                "HandlerService",
                "LoggerService",
                "StatsService",
                "RoutingService",
            ],
        }),
    );
    document.insert("stats".into(), json!({}));
    let mut inbounds = object_array(document.remove("inbounds"), Some("sempre-api-in"));
    inbounds.push(json!({
        "tag": "sempre-api-in",
        "listen": "127.0.0.1",
        "port": port,
        "protocol": "dokodemo-door",
        "settings": {"address": "127.0.0.1"},
    }));
    document.insert("inbounds".into(), Value::Array(inbounds));

    let routing = object_mut(document.entry("routing").or_insert(Value::Null));
    let mut rules = object_array(routing.remove("rules"), None);
    rules.insert(
        0,
        json!({
            "type": "field",
            "inboundTag": ["sempre-api-in"],
            "outboundTag": "sempre-api",
        }),
    );
    routing.insert("rules".into(), Value::Array(rules));

    let encoded = serde_json::to_vec_pretty(&document)
        .map_err(|error| runtime_error(core, format!("encode configuration: {error}")))?;
    let path = write_runtime(core, directory, "config.json", &encoded, true)?;
    Ok(RuntimeSpec {
        config: path,
        control: Some(control),
    })
}

fn private_control(core: &str, protocol: ControlProtocol) -> Result<ControlSpec, RegistryError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| runtime_error(core, format!("reserve control address: {error}")))?;
    let address = listener
        .local_addr()
        .map_err(|error| runtime_error(core, format!("read control address: {error}")))?;
    drop(listener);
    let bytes: [u8; 32] = random();
    let mut secret = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        secret.push(char::from(HEX[usize::from(byte >> 4)]));
        secret.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(ControlSpec {
        core: core.into(),
        protocol,
        base_url: format!("http://{address}"),
        secret,
    })
}

fn read_json_object(core: &str, path: &Path) -> Result<Map<String, Value>, RegistryError> {
    let data = fs::read(path)
        .map_err(|error| runtime_error(core, format!("read configuration: {error}")))?;
    serde_json::from_slice::<Value>(&data)
        .map_err(|error| runtime_error(core, format!("decode configuration: {error}")))?
        .as_object()
        .cloned()
        .ok_or_else(|| runtime_error(core, "configuration root must be an object"))
}

fn object_mut(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = json!({});
    }
    value
        .as_object_mut()
        .expect("value was replaced with object")
}

fn object_array(value: Option<Value>, excluded_tag: Option<&str>) -> Vec<Value> {
    value
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|item| {
            excluded_tag.is_none_or(|tag| item.get("tag").and_then(Value::as_str) != Some(tag))
        })
        .collect()
}

fn write_runtime(
    core: &str,
    directory: &Path,
    name: &str,
    data: &[u8],
    trailing_newline: bool,
) -> Result<PathBuf, RegistryError> {
    fs::create_dir_all(directory)
        .map_err(|error| runtime_error(core, format!("create runtime directory: {error}")))?;
    set_directory_permissions(core, directory)?;
    let path = directory.join(name);
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|error| runtime_error(core, format!("open runtime configuration: {error}")))?;
    file.write_all(data)
        .and_then(|()| trailing_newline.then(|| file.write_all(b"\n")).transpose())
        .map_err(|error| runtime_error(core, format!("write runtime configuration: {error}")))?;
    set_file_permissions(core, &path)?;
    Ok(path)
}

#[cfg(unix)]
fn set_directory_permissions(core: &str, path: &Path) -> Result<(), RegistryError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| runtime_error(core, format!("secure runtime directory: {error}")))
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn set_directory_permissions(_: &str, _: &Path) -> Result<(), RegistryError> {
    Ok(())
}

#[cfg(unix)]
fn set_file_permissions(core: &str, path: &Path) -> Result<(), RegistryError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| runtime_error(core, format!("secure runtime configuration: {error}")))
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn set_file_permissions(_: &str, _: &Path) -> Result<(), RegistryError> {
    Ok(())
}

fn control_address(control: &ControlSpec) -> &str {
    control
        .base_url
        .strip_prefix("http://")
        .expect("private control URL always uses HTTP")
}

const fn core_id(kind: BuiltInKind) -> &'static str {
    match kind {
        BuiltInKind::SingBox => "sing-box",
        BuiltInKind::Mihomo => "mihomo",
        BuiltInKind::Xray => "xray",
        BuiltInKind::V2Ray => "v2ray",
        BuiltInKind::ClashRs => "clash-rs",
        BuiltInKind::Dae => "dae",
    }
}

fn runtime_error(core: &str, message: impl Into<String>) -> RegistryError {
    RegistryError::Runtime {
        core: core.into(),
        message: message.into(),
    }
}
