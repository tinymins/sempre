use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DomesticDomainMode {
    #[default]
    Direct,
    Proxy,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct DomesticDomainPolicy {
    pub enabled: bool,
    pub mode: DomesticDomainMode,
}

impl Default for DomesticDomainPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: DomesticDomainMode::Direct,
        }
    }
}
