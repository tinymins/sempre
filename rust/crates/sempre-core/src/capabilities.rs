use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Capabilities {
    pub features: Vec<String>,
    pub enum_values: BTreeMap<String, Vec<String>>,
    pub protocols: Vec<ProtocolCapability>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProtocolCapability {
    pub protocol: String,
    pub transports: Vec<String>,
    pub security: Vec<String>,
    pub minimum_version: Option<String>,
}

impl Capabilities {
    #[must_use]
    pub fn normalize(mut self) -> Self {
        normalize_strings(&mut self.features);
        for values in self.enum_values.values_mut() {
            normalize_strings(values);
        }
        for protocol in &mut self.protocols {
            normalize_strings(&mut protocol.transports);
            normalize_strings(&mut protocol.security);
        }
        self.protocols
            .sort_by(|left, right| left.protocol.cmp(&right.protocol));
        self.protocols
            .dedup_by(|left, right| left.protocol == right.protocol);
        self
    }

    pub fn intersection(values: impl IntoIterator<Item = Self>) -> Self {
        let mut values = values.into_iter();
        let Some(mut result) = values.next().map(Self::normalize) else {
            return Self::default();
        };
        for next in values.map(Self::normalize) {
            result.features = intersect(&result.features, &next.features);
            result.enum_values.retain(|key, current| {
                next.enum_values.get(key).is_some_and(|other| {
                    *current = intersect(current, other);
                    true
                })
            });
            result.protocols.retain_mut(|protocol| {
                let Some(other) = next
                    .protocols
                    .iter()
                    .find(|item| item.protocol == protocol.protocol)
                else {
                    return false;
                };
                protocol.transports = intersect(&protocol.transports, &other.transports);
                protocol.security = intersect(&protocol.security, &other.security);
                if other.minimum_version > protocol.minimum_version {
                    protocol.minimum_version.clone_from(&other.minimum_version);
                }
                true
            });
        }
        result.normalize()
    }
}

fn normalize_strings(values: &mut Vec<String>) {
    values.retain(|value| !value.is_empty());
    values.sort();
    values.dedup();
}

fn intersect(left: &[String], right: &[String]) -> Vec<String> {
    let right = right.iter().collect::<BTreeSet<_>>();
    left.iter()
        .filter(|value| right.contains(value))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_keeps_only_shared_normalized_features() {
        let left = Capabilities {
            features: vec!["tun".into(), "dns".into(), "dns".into()],
            ..Capabilities::default()
        };
        let right = Capabilities {
            features: vec!["dns".into(), "tproxy".into()],
            ..Capabilities::default()
        };
        assert_eq!(Capabilities::intersection([left, right]).features, ["dns"]);
    }
}
