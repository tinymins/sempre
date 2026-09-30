use crate::ProxyGroup;

pub(super) const PRIVATE: &str = "内网地址";
pub(super) const CHINA: &str = "中国地址";
const FOREIGN: &str = "🔰 国外流量";

/// Add routing selectors after the renderer has established its fallback group.
pub(super) fn append_missing(groups: &mut Vec<ProxyGroup>) {
    let mut members = vec!["DIRECT".into()];
    if groups.iter().any(|group| group.name == FOREIGN) {
        members.push(FOREIGN.into());
    }
    for name in [PRIVATE, CHINA] {
        if groups.iter().any(|group| group.name == name) {
            continue;
        }
        groups.push(ProxyGroup {
            name: name.into(),
            group_type: "select".into(),
            proxies: members.clone(),
            include_all: true,
            default: "DIRECT".into(),
            ..ProxyGroup::default()
        });
    }
}
