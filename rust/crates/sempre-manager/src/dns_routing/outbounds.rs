use serde_json::Value;

use crate::ManagerError;

pub(super) fn compiled_proxy_names(document: &Value) -> Result<Vec<String>, ManagerError> {
    let outbounds = document
        .get("outbounds")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ManagerError::InvalidOperation("remote sing-box configuration has no outbounds".into())
        })?;
    Ok(outbounds
        .iter()
        .filter_map(|outbound| {
            let kind = outbound.get("type")?.as_str()?;
            let tag = outbound.get("tag")?.as_str()?;
            (!matches!(kind, "direct" | "block" | "dns" | "selector" | "urltest"))
                .then(|| tag.to_owned())
        })
        .collect())
}

pub(super) fn compiled_direct_outbound(document: &Value) -> Result<String, ManagerError> {
    let outbounds = document["outbounds"]
        .as_array()
        .expect("validated outbounds");
    let direct = outbounds
        .iter()
        .filter(|outbound| outbound["type"] == "direct")
        .filter_map(|outbound| outbound["tag"].as_str())
        .collect::<Vec<_>>();
    match direct.as_slice() {
        [tag] => Ok((*tag).into()),
        _ => Err(ManagerError::InvalidOperation(
            "remote sing-box configuration must have one direct outbound".into(),
        )),
    }
}
