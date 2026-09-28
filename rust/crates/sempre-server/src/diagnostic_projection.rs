use std::collections::HashMap;

use sempre_converter::{
    CompileRequest, CompileResult, ParseResult, parse_subscription, prepare_profile, preview_nodes,
    rule_provider_snapshot_id,
};
use serde_json::{Value, json};

use crate::debug_stream::StageLog;

pub(crate) fn project(
    request: &CompileRequest,
    result: &CompileResult,
    stages: &mut StageLog,
) -> (Vec<Value>, Vec<Value>) {
    let samples = rule_samples(request, &result.content);
    let parsed = request
        .snapshots
        .iter()
        .filter(|snapshot| {
            request
                .profile
                .sources
                .iter()
                .any(|source| source.id == snapshot.source_id)
        })
        .map(|snapshot| {
            (
                snapshot.source_id.as_str(),
                parse_subscription(&snapshot.content),
            )
        })
        .collect::<HashMap<_, _>>();
    let preview = match preview_nodes(request) {
        Ok(nodes) => nodes,
        Err(error) => {
            stages.push(json!({"type":"inspection","status":"error","message":error.to_string()}));
            return (Vec::new(), samples);
        }
    };
    let filters = match prepare_profile(&request.profile, &request.target) {
        Ok(profile) => profile.filters,
        Err(error) => {
            stages.push(json!({"type":"inspection","status":"error","message":error.to_string()}));
            return (Vec::new(), samples);
        }
    };
    for source in &request.profile.sources {
        let before = parsed
            .get(source.id.as_str())
            .map_or(0, |parsed| parsed.nodes.len());
        let after = result
            .node_origins
            .values()
            .filter(|origin| *origin == &format!("source:{}", source.id))
            .count();
        let index = source_index(stages, &source.id);
        stages.push(
            json!({"type":"source-filter","status":"ok","sourceId":source.id,
            "sourceIndex":index,"nodesBeforeFilter":before,"nodesAfterFilter":after,
            "filteredCount":before.saturating_sub(after)}),
        );
    }
    let mut traces = result.field_diffs.iter().enumerate().map(|(position, diff)| {
        let source_id = result.node_origins.get(&diff.node).and_then(|origin| origin.strip_prefix("source:"));
        let index = source_id.map_or(0, |id| source_index(stages, id));
        let format = source_id.and_then(|id| parsed.get(id)).map_or("manual", |parsed| parsed.format.as_str());
        let mut steps = vec![
            json!({"type":"source","data":{"sourceIndex":index,"sourceId":source_id,"format":format}}),
            json!({"type":"filter","data":{"passed":true,"matchedRule":null}}),
            json!({"type":"merge","data":{"positionInCompiledList":position + 1,
                "totalCompiledNodes":result.field_diffs.len()}}),
        ];
        if diff.outbound.is_some() || !diff.dropped.is_empty() || !diff.ignored.is_empty() {
            steps.push(json!({"type":"convert","data":{
                "singboxOutbound":diff.outbound,"lostFields":diff.dropped,
                "ignoredFields":diff.ignored,"consumedFields":diff.consumed}}));
        }
        steps.push(json!({"type":"output","data":{
            "configFragment":diff.outbound.as_ref().map(Value::to_string),
            "represented":diff.represented,"fieldDiff":diff}}));
        json!({"traceId":position.to_string(),"nodeName":diff.node,
            "sourceId":source_id,"sourceIndex":index,"steps":steps})
    }).collect::<Vec<_>>();
    for node in &preview {
        // The compiler filters URL-source nodes by their prefixed name before adding icons.
        if node.source_index == 0 {
            continue;
        }
        let Some(source) = request.profile.sources.get(node.source_index - 1) else {
            continue;
        };
        let Some(matched_rule) = filters
            .iter()
            .find(|filter| !filter.is_empty() && node.original_name.contains(filter.as_str()))
        else {
            continue;
        };
        let index = source_index(stages, &source.id);
        let trace_id = traces.len().to_string();
        traces.push(json!({"traceId":trace_id,"nodeName":node.name,"sourceId":source.id,
            "sourceIndex":index,"filtered":true,"steps":[
                {"type":"source","data":{"sourceIndex":index,"sourceId":source.id,
                    "format":parsed.get(source.id.as_str()).map_or("unknown", |parsed: &ParseResult| parsed.format.as_str())}},
                {"type":"filter","data":{"passed":false,"matchedRule":matched_rule,
                    "nameBeforeIcon":node.original_name}},
            ]}));
    }
    stages.push(json!({"type":"merge","status":"ok","nodeCount":result.field_diffs.len(),
        "representedCount":result.node_count,"filteredCount":traces.len().saturating_sub(result.field_diffs.len())}));
    stages.push(json!({"type":"output","status":"ok","format":result.format,
        "nodeCount":result.node_count,"runtimeValidated":result.runtime_validated,
        "contentBytes":result.content.len(),"ruleSections":samples.len()}));
    (traces, samples)
}

fn source_index(stages: &StageLog, source_id: &str) -> u64 {
    stages
        .iter()
        .find(|stage| {
            stage.get("type").and_then(Value::as_str) == Some("fetch")
                && stage.get("sourceId").and_then(Value::as_str) == Some(source_id)
        })
        .and_then(|stage| stage.get("sourceIndex"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn rule_samples(request: &CompileRequest, content: &str) -> Vec<Value> {
    let mut samples = Vec::new();
    if let Ok(decoded) = serde_yaml::from_str::<Value>(content) {
        for (section, value) in [
            ("rules", decoded.get("rules")),
            ("route.rules", decoded.pointer("/route/rules")),
            ("route.rule_set", decoded.pointer("/route/rule_set")),
            ("rule-providers", decoded.get("rule-providers")),
        ] {
            if let Some(value) = value {
                samples.push(json!({"section":section,"lines":lines(value)}));
            }
        }
    }
    let effective = prepare_profile(&request.profile, &request.target).ok();
    let providers = effective
        .as_ref()
        .map_or(&request.profile.rule_providers, |profile| {
            &profile.rule_providers
        })
        .iter()
        .map(|provider| {
            (
                rule_provider_snapshot_id(&provider.tag),
                provider.tag.as_str(),
            )
        })
        .collect::<HashMap<_, _>>();
    for snapshot in &request.snapshots {
        if let Some(tag) = providers.get(&snapshot.source_id) {
            samples.push(json!({"section":format!("rule provider: {tag}"),
                "lines":snapshot.content.lines().map(str::to_owned).collect::<Vec<_>>() }));
        }
    }
    samples
}

fn lines(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items.iter().map(Value::to_string).collect(),
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| format!("{key}: {value}"))
            .collect(),
        _ => vec![value.to_string()],
    }
}
