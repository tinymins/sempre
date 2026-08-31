use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingConfigField {
    Sources,
    SubscriptionContent,
    Nodes,
    Groups,
    Rules,
    RuleProviders,
    Filters,
    Dns,
    PrivateAccess,
    LocalProxy,
    TransparentProxy,
    ManagementApi,
    Advanced,
    ManualConfiguration,
}
