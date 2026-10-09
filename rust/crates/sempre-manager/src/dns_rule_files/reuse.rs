use std::fs;

use sempre_state::{Document, Layout};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{DnsSettings, Manager, ManagerError, VersionRunner, subscription::RenderedProfile};

use super::{ArtifactSnapshot, decode, prepare_update, projection, read};

impl<R: VersionRunner> Manager<R> {
    pub(crate) fn reuse_applied_dns_rules(
        &self,
        document: &Document,
        rendered: &mut RenderedProfile,
    ) -> Result<bool, ManagerError> {
        if rendered.target.core != "sing-box" {
            return Ok(false);
        }
        let Some((content, hash)) = reuse_runtime_domains(
            self.store.layout(),
            document,
            &self.dns_settings.read(),
            &rendered.render.content,
        )?
        else {
            return Ok(false);
        };
        rendered.render.content = content;
        rendered.render.artifact_hash = hash;
        Ok(true)
    }
}

fn reuse_runtime_domains(
    layout: &Layout,
    document: &Document,
    saved: &DnsSettings,
    content: &str,
) -> Result<Option<(String, String)>, ManagerError> {
    if saved.rule_sets.is_empty() {
        return Ok(None);
    }
    let Some(hash) = document.configs.get("sing-box") else {
        return Ok(None);
    };
    let artifact = layout.config("sing-box", hash);
    let metadata = match fs::read(artifact.with_extension("dns-rule-snapshot.json")) {
        Ok(bytes) => decode::<ArtifactSnapshot>(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ManagerError::io("read DNS rule artifact snapshot", error)),
    };
    if !metadata.trusted || !projection::same_structure(&metadata.settings, saved) {
        return Ok(None);
    }
    let old_data = read(&artifact)?;
    if old_data == content.as_bytes() || format!("{:x}", Sha256::digest(&old_data)) != *hash {
        return Ok(None);
    }
    let old: Value = decode(&old_data)?;
    let mut candidate: Value = decode(content.as_bytes())?;
    if !projection::matches_inline(&old, &metadata.settings)
        || !projection::matches_inline(&candidate, saved)
    {
        return Ok(None);
    }
    let definitions = candidate["route"]["rule_set"]
        .as_array_mut()
        .expect("validated managed rule definitions");
    for rule_set in &metadata.settings.rule_sets {
        let definition = definitions
            .iter_mut()
            .find(|value| value["tag"] == projection::tag(rule_set))
            .expect("validated managed rule definition");
        definition["rules"] = serde_json::json!([crate::dns_routing::inline_rule(rule_set)]);
    }
    if candidate != old {
        return Ok(None);
    }
    let Some(update) = prepare_update(layout, document, saved, saved)? else {
        return Ok(None);
    };
    if !update.files.is_empty() {
        return Ok(None);
    }
    let old_content = String::from_utf8(old_data).map_err(|error| {
        ManagerError::InvalidOperation(format!("decode stored configuration: {error}"))
    })?;
    Ok(Some((old_content, hash.clone())))
}
