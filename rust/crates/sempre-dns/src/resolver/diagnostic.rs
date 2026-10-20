use std::net::IpAddr;

use serde::Serialize;

use super::Resolver;
use crate::{
    DnsError,
    dns_wire::{answer_ip_addresses, build_query, record_number, response_code},
};

/// A handle to the running resolver, not a reconstructed saved configuration.
#[derive(Clone)]
pub struct DnsDiagnosticHandle(pub(crate) Resolver);

#[derive(Serialize)]
pub struct DnsDiagnosticResult {
    pub addresses: Vec<IpAddr>,
    pub response_code: u8,
    pub upstream: String,
    pub detail: String,
    pub steps: Vec<String>,
}

impl DnsDiagnosticHandle {
    /// Evaluates only domain predicates; never sends a DNS query.
    pub fn preview(&self, name: &str) -> Vec<String> {
        let state = self.0.snapshot();
        let mut steps = vec!["Active frontend rules; prediction only, no DNS query sent".into()];
        if let Some(rewrite) = self.0.policy.rewrite(name, "A") {
            steps.push(format!("A rewrite:{} -> {}", rewrite.id, rewrite.answer));
            return steps;
        }
        steps.push("A rewrite: no match".into());
        for rule in &state.rule_sets {
            let matched = rule.matcher.matches(name);
            steps.push(format!(
                "rule-set:{} {}",
                rule.id,
                if matched { "matched" } else { "not matched" }
            ));
            if matched {
                steps.push(format!("Predicted DNS upstream: {}", rule.upstream));
                return steps;
            }
        }
        steps.push(if state.config.strategy == "rules-first" {
            "Predicted DNS upstream: remote".into()
        } else {
            "Upstream depends on actual local DNS response; awaiting test".into()
        });
        steps
    }

    /// Executes the same resolver branches and upstream exchanges used by the listener.
    /// Calling this does not demonstrate that OS DNS interception is working.
    pub async fn resolve_with_progress<F>(
        &self,
        name: &str,
        record_type: &str,
        mut on_progress: F,
    ) -> Result<DnsDiagnosticResult, DnsError>
    where
        F: FnMut(String) + Send,
    {
        let (_, number) = record_number(record_type)?;
        let packet = build_query(name, number)?;
        let mut steps = Vec::new();
        let resolved = self
            .0
            .snapshot()
            .resolve_observed(
                &packet,
                self.0.policy.as_ref(),
                Some(&mut |step: String| {
                    steps.push(step.clone());
                    on_progress(step);
                }),
            )
            .await?;
        Ok(DnsDiagnosticResult {
            addresses: answer_ip_addresses(&resolved.packet)?,
            response_code: response_code(&resolved.packet)?,
            upstream: resolved.upstream,
            detail: resolved.detail,
            steps,
        })
    }
}
