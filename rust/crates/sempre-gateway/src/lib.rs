mod controller;
mod dhcp;
mod dns;
mod dns_wire;
mod domain_matcher;
mod error;
mod frontend_config;
mod frontend_service;
mod host;
mod model;
mod probe;
mod rules;
mod store;

pub use controller::Controller;
pub use dns::{DnsDebugResult, managed_probe_names};
pub use domain_matcher::{
    DOMESTIC_DOMAIN_COUNT, DOMESTIC_DOMAIN_SHA256, DOMESTIC_DOMAIN_SOURCE, bundled_domestic_domains,
};
pub use error::GatewayError;
pub use frontend_service::DnsService;
pub use host::{HostApplyRequest, HostPlan, HostPlanRequest, apply_host_plan, build_host_plan};
pub use model::{
    Config, DnsConfig, DnsRuleSet, LeaseView, RuntimeStatus, Status, validation_messages,
};
pub use probe::{DnsProbeResult, probe_dns};
pub use store::Store;
