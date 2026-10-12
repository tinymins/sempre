use crate::Profile;

/// Host routing is a daemon policy, carried through compilation without IO.
pub fn cn_direct_routes(profile: &Profile) -> bool {
    profile
        .extra
        .get("sempre_cn_direct_routes")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}
