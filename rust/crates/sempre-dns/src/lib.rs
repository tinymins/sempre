mod dns;
mod dns_policy;
mod dns_wire;
mod domain_matcher;
mod error;
mod frontend_config;
mod frontend_service;
mod model;
mod probe;
mod resolver;
mod socket;

pub use dns::{DnsDebugResult, debug_query, managed_probe_names};
pub use dns_policy::{DnsQueryEvent, DnsRewrite, DnsRuntimePolicy};
pub use domain_matcher::{
    DOMESTIC_DOMAIN_COUNT, DOMESTIC_DOMAIN_SHA256, DOMESTIC_DOMAIN_SOURCE, bundled_domestic_domains,
};
pub use error::DnsError;
pub use frontend_service::DnsService;
pub use model::{DnsConfig, DnsRuleSet};
pub use probe::{DnsProbeResult, probe_dns};
