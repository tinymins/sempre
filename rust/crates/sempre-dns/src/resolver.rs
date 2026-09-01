use std::{
    net::Ipv4Addr,
    sync::{Arc, RwLock},
    time::Duration,
};

use chrono::Utc;
use tokio::{net::lookup_host, time::Instant, time::timeout};

use crate::{
    DnsError,
    dns::DnsDebugResult,
    dns_policy::{DnsQueryEvent, DnsRuntimePolicy},
    dns_wire::{
        TYPE_HTTPS, answer_ipv4_addresses, build_query, format_answers, fqdn, parse_question,
        record_number, response_with_answer, response_with_code, type_name,
    },
    domain_matcher::DomainMatcher,
    model::DnsConfig,
};

#[derive(Clone)]
pub(crate) struct Resolver {
    state: Arc<RwLock<Arc<ResolverState>>>,
    policy: Arc<dyn DnsRuntimePolicy>,
}

struct ResolverState {
    config: DnsConfig,
    domestic_cidrs: Vec<(Ipv4Addr, u8)>,
    rule_sets: Vec<ResolvedRuleSet>,
}

#[derive(Clone)]
struct ResolvedRuleSet {
    id: String,
    upstream: String,
    matcher: DomainMatcher,
}

struct Resolved {
    packet: Vec<u8>,
    upstream: String,
    detail: String,
}

impl Resolver {
    pub(crate) fn new(
        config: DnsConfig,
        policy: Arc<dyn DnsRuntimePolicy>,
    ) -> Result<Self, DnsError> {
        Ok(Self {
            state: Arc::new(RwLock::new(Arc::new(ResolverState::new(config)?))),
            policy,
        })
    }

    pub(crate) fn update(&self, config: DnsConfig) -> Result<(), DnsError> {
        let next = Arc::new(ResolverState::new(config)?);
        let mut state = self.state.write().expect("DNS resolver state");
        if state.config.listen_hosts != next.config.listen_hosts
            || state.config.listen_port != next.config.listen_port
            || state.config.outbound_mark != next.config.outbound_mark
        {
            return Err(DnsError::invalid(
                "DNS listener changes require restarting the service",
            ));
        }
        *state = next;
        Ok(())
    }

    pub(crate) fn config(&self) -> DnsConfig {
        self.snapshot().config.clone()
    }

    pub(crate) async fn debug_query(
        &self,
        name: &str,
        record_type: &str,
    ) -> Result<DnsDebugResult, DnsError> {
        self.snapshot()
            .debug_query(name, record_type, self.policy.as_ref())
            .await
    }

    pub(crate) async fn resolve_for_client(&self, packet: &[u8], client: String) -> Vec<u8> {
        self.snapshot()
            .resolve_for_client(packet, client, self.policy.as_ref())
            .await
    }

    pub(crate) fn selected_upstream(&self, name: &str) -> String {
        self.snapshot().selected_upstream(name).into()
    }

    fn snapshot(&self) -> Arc<ResolverState> {
        Arc::clone(&self.state.read().expect("DNS resolver state"))
    }
}

impl ResolverState {
    fn new(config: DnsConfig) -> Result<Self, DnsError> {
        let domestic_cidrs = config
            .domestic_cidrs
            .iter()
            .map(|value| parse_cidr(value))
            .collect::<Result<Vec<_>, _>>()?;
        let rule_sets = config
            .rule_sets
            .iter()
            .filter(|rule_set| rule_set.enabled)
            .map(|rule_set| ResolvedRuleSet {
                id: rule_set.id.clone(),
                upstream: rule_set.upstream.clone(),
                matcher: DomainMatcher::from_rules(&rule_set.rules),
            })
            .collect();
        Ok(Self {
            config,
            domestic_cidrs,
            rule_sets,
        })
    }

    async fn debug_query(
        &self,
        name: &str,
        record_type: &str,
        policy: &dyn DnsRuntimePolicy,
    ) -> Result<DnsDebugResult, DnsError> {
        let (record_type, number) = record_number(record_type)?;
        let packet = build_query(name, number)?;
        let resolved = self.resolve(&packet, policy).await?;
        Ok(DnsDebugResult {
            name: fqdn(name),
            record_type,
            upstream: resolved.upstream,
            answers: format_answers(&resolved.packet)?,
            detail: resolved.detail,
        })
    }

    async fn resolve(
        &self,
        packet: &[u8],
        policy: &dyn DnsRuntimePolicy,
    ) -> Result<Resolved, DnsError> {
        let question = parse_question(packet)?;
        let Some(question) = question else {
            return Ok(Resolved {
                packet: response_with_code(packet, 0)?,
                upstream: String::new(),
                detail: "empty-question".into(),
            });
        };
        let record_type = type_name(question.record_type);
        if let Some(rewrite) = policy.rewrite(&question.name, record_type) {
            return Ok(Resolved {
                packet: response_with_answer(
                    packet,
                    question.record_type,
                    &rewrite.answer,
                    rewrite.ttl,
                )?,
                upstream: "rewrite".into(),
                detail: format!("rewrite:{}", rewrite.id),
            });
        }
        if (self.config.reject_https || policy.reject_https()) && question.record_type == TYPE_HTTPS
        {
            return Ok(Resolved {
                packet: response_with_code(packet, 3)?,
                upstream: "reject".into(),
                detail: "https-rejected".into(),
            });
        }
        if let Some(rule_set) = self.match_rule(&question.name) {
            let mut resolved = self.exchange_named(packet, &rule_set.upstream).await?;
            resolved.detail = format!("rule-set:{}", rule_set.id);
            return Ok(resolved);
        }
        if self.config.strategy == "rules-first" {
            let mut resolved = self.exchange(packet, &self.config.remote_upstream).await?;
            resolved.detail = "default-remote".into();
            return Ok(resolved);
        }
        let local = self.exchange_local(packet).await;
        if local
            .as_ref()
            .is_ok_and(|response| self.domestic_response(&response.packet))
        {
            return local.map(|mut response| {
                response.detail = "local-response".into();
                response
            });
        }
        match self.exchange(packet, &self.config.remote_upstream).await {
            Ok(mut remote) => {
                remote.detail = "remote-response".into();
                Ok(remote)
            }
            Err(remote_error) => local.or(Err(remote_error)),
        }
    }

    async fn resolve_for_client(
        &self,
        packet: &[u8],
        client: String,
        policy: &dyn DnsRuntimePolicy,
    ) -> Vec<u8> {
        let started = Instant::now();
        let question = parse_question(packet).ok().flatten();
        let result = self.resolve(packet, policy).await;
        let (response, upstream, detail, error) = match result {
            Ok(value) => (value.packet, value.upstream, value.detail, String::new()),
            Err(error) => (
                response_with_code(packet, 2).unwrap_or_default(),
                String::new(),
                "error".into(),
                error.to_string(),
            ),
        };
        let decision = if detail.starts_with("rewrite:") {
            "rewrite"
        } else if detail == "https-rejected" {
            "reject"
        } else if detail.contains("local") || self.config.local_upstreams.contains(&upstream) {
            "local"
        } else if detail == "error" {
            "error"
        } else {
            "core"
        };
        policy.record(DnsQueryEvent {
            time: Utc::now().timestamp_millis(),
            client,
            name: question
                .as_ref()
                .map_or_else(String::new, |value| value.name.clone()),
            record_type: question
                .as_ref()
                .map_or_else(String::new, |value| type_name(value.record_type).into()),
            decision: decision.into(),
            answers: format_answers(&response).unwrap_or_default(),
            upstream,
            latency_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            detail,
            error,
        });
        response
    }

    async fn exchange_named(&self, packet: &[u8], upstream: &str) -> Result<Resolved, DnsError> {
        match upstream {
            "local" => self.exchange_local(packet).await,
            "remote" | "" => self.exchange(packet, &self.config.remote_upstream).await,
            upstream => self.exchange(packet, upstream).await,
        }
    }

    async fn exchange_local(&self, packet: &[u8]) -> Result<Resolved, DnsError> {
        if self.config.local_upstreams.is_empty() {
            return self.exchange(packet, &self.config.remote_upstream).await;
        }
        let mut last_error = None;
        for upstream in &self.config.local_upstreams {
            match self.exchange(packet, upstream).await {
                Ok(response) => return Ok(response),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.expect("non-empty local upstreams"))
    }

    async fn exchange(&self, packet: &[u8], upstream: &str) -> Result<Resolved, DnsError> {
        let address = lookup_host(upstream)
            .await
            .map_err(|error| DnsError::io(format!("resolve DNS upstream {upstream}"), error))?
            .next()
            .ok_or_else(|| DnsError::invalid(format!("DNS upstream {upstream:?} is empty")))?;
        let socket = crate::socket::upstream_socket(self.config.outbound_mark).await?;
        socket
            .connect(address)
            .await
            .map_err(|error| DnsError::io(format!("connect DNS upstream {upstream}"), error))?;
        socket
            .send(packet)
            .await
            .map_err(|error| DnsError::io(format!("send DNS query to {upstream}"), error))?;
        let mut response = vec![0_u8; u16::MAX as usize];
        let count = timeout(Duration::from_secs(5), socket.recv(&mut response))
            .await
            .map_err(|_| DnsError::invalid(format!("DNS upstream {upstream} timed out")))?
            .map_err(|error| {
                DnsError::io(format!("receive DNS response from {upstream}"), error)
            })?;
        response.truncate(count);
        if response.get(..2) != packet.get(..2) {
            return Err(DnsError::invalid(format!(
                "DNS upstream {upstream} returned a mismatched transaction"
            )));
        }
        Ok(Resolved {
            packet: response,
            upstream: upstream.into(),
            detail: String::new(),
        })
    }

    fn match_rule(&self, name: &str) -> Option<&ResolvedRuleSet> {
        self.rule_sets
            .iter()
            .find(|rule_set| rule_set.matcher.matches(name))
    }

    fn selected_upstream(&self, name: &str) -> &str {
        self.match_rule(name)
            .map_or("remote", |rule_set| rule_set.upstream.as_str())
    }

    fn domestic_response(&self, packet: &[u8]) -> bool {
        answer_ipv4_addresses(packet).is_ok_and(|addresses| {
            addresses.into_iter().any(|address| {
                self.domestic_cidrs
                    .iter()
                    .any(|(network, prefix)| in_prefix(address, *network, *prefix))
            })
        })
    }
}

fn parse_cidr(value: &str) -> Result<(Ipv4Addr, u8), DnsError> {
    let (address, prefix) = value
        .split_once('/')
        .ok_or_else(|| DnsError::invalid(format!("invalid IPv4 prefix {value:?}")))?;
    let address = address
        .parse()
        .map_err(|_| DnsError::invalid(format!("invalid IPv4 prefix {value:?}")))?;
    let prefix = prefix
        .parse::<u8>()
        .ok()
        .filter(|prefix| *prefix <= 32)
        .ok_or_else(|| DnsError::invalid(format!("invalid IPv4 prefix {value:?}")))?;
    Ok((address, prefix))
}

fn in_prefix(address: Ipv4Addr, network: Ipv4Addr, prefix: u8) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    u32::from(address) & mask == u32::from(network) & mask
}
