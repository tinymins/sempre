mod editor;
mod model;
mod parser;
mod renderer;
mod target;

pub use model::{
    CompileRequest, CompileResult, CustomNode, Diagnostic, FieldDiff, Profile, Proxy, ProxyGroup,
    SourceSnapshot,
};
pub use parser::{ParseResult, parse_subscription};
pub use target::{Target, available_targets};

use std::collections::{HashMap, HashSet};

use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("unsupported output format {0:?}")]
    UnsupportedTarget(String),
    #[error("source {0:?} has no supplied snapshot")]
    MissingSnapshot(String),
    #[error("source {label:?} produced no usable nodes: {detail}")]
    EmptySource { label: String, detail: String },
    #[error("subscription profile produced no usable nodes")]
    EmptyProfile,
    #[error("invalid custom node {0:?}")]
    InvalidCustomNode(String),
    #[error("invalid editor field {field:?}: {detail}")]
    InvalidEditor { field: &'static str, detail: String },
    #[error("render output failed: {0}")]
    Render(String),
}

pub fn compile(request: &CompileRequest) -> Result<CompileResult, CompileError> {
    let profile = editor::apply(&request.profile)?;
    let mut target = Target::parse(&request.target.format)?;
    if !request.target.core.trim().is_empty() {
        target.core.clone_from(&request.target.core);
    }
    let snapshots: HashMap<&str, &SourceSnapshot> = request
        .snapshots
        .iter()
        .map(|snapshot| (snapshot.source_id.as_str(), snapshot))
        .collect();
    let mut nodes = Vec::new();
    let mut diagnostics = Vec::new();

    for value in &profile.manual_servers {
        let proxy = Proxy::from_value(value.clone())
            .map_err(|_| CompileError::InvalidCustomNode("manual server".into()))?;
        nodes.push((proxy, "manual-server".into()));
    }
    for source in profile.sources.iter().filter(|source| source.enabled) {
        let snapshot = snapshots
            .get(source.id.as_str())
            .ok_or_else(|| CompileError::MissingSnapshot(source.id.clone()))?;
        let parsed = parse_subscription(&snapshot.content);
        if parsed.nodes.is_empty() {
            return Err(CompileError::EmptySource {
                label: source.label(),
                detail: parsed.messages().join("; "),
            });
        }
        diagnostics.extend(parsed.diagnostics.into_iter().map(|message| Diagnostic {
            level: "warning".into(),
            source_id: Some(source.id.clone()),
            message,
        }));
        for mut proxy in parsed.nodes {
            if !source.prefix.trim().is_empty() {
                proxy.name = format!("{}{}", normalize_prefix(&source.prefix), proxy.name);
            }
            nodes.push((proxy, format!("source:{}", source.id)));
        }
    }

    let selected: HashSet<&str> = profile.custom_node_ids.iter().map(String::as_str).collect();
    for node in request
        .custom_nodes
        .iter()
        .filter(|node| selected.contains(node.id.as_str()))
    {
        let proxy = Proxy::from_value(node.proxy.clone())
            .map_err(|_| CompileError::InvalidCustomNode(node.name.clone()))?;
        nodes.push((proxy, format!("custom-node:{}", node.id)));
    }

    apply_filters(&mut nodes, &profile.filters);
    make_names_unique(&mut nodes);
    if nodes.is_empty() {
        return Err(CompileError::EmptyProfile);
    }
    let mut origins = HashMap::new();
    let proxies: Vec<Proxy> = nodes
        .into_iter()
        .map(|(proxy, origin)| {
            origins.insert(proxy.name.clone(), origin);
            proxy
        })
        .collect();

    let (content, field_diffs, warnings) = renderer::render(&profile, &proxies, &target)?;
    diagnostics.extend(warnings.into_iter().map(|message| Diagnostic {
        level: "warning".into(),
        source_id: None,
        message,
    }));
    let artifact_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
    Ok(CompileResult {
        protocol: 1,
        format: target.format,
        version: target.version,
        platform: target.platform,
        content,
        artifact_hash,
        node_count: field_diffs.iter().filter(|diff| diff.represented).count(),
        field_diffs,
        node_origins: origins,
        diagnostics,
        runtime_validated: false,
    })
}

fn normalize_prefix(value: &str) -> String {
    let value = value.trim();
    if value.ends_with([' ', '-', '_', '|', ':']) {
        value.into()
    } else {
        format!("{value} ")
    }
}

fn apply_filters(nodes: &mut Vec<(Proxy, String)>, filters: &[String]) {
    nodes.retain(|(proxy, origin)| {
        !origin.starts_with("source:")
            || !filters
                .iter()
                .any(|filter| !filter.is_empty() && proxy.name.contains(filter))
    });
}

fn make_names_unique(nodes: &mut [(Proxy, String)]) {
    let mut counts = HashMap::<String, usize>::new();
    for (proxy, _) in nodes {
        let count = counts.entry(proxy.name.clone()).or_default();
        *count += 1;
        if *count > 1 {
            proxy.name = format!("{} ({})", proxy.name, count);
        }
    }
}
