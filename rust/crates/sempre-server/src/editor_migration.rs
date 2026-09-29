//! One-time conversion of the old Server defaults into ordinary custom settings.
use sempre_converter::parse_jsonc_value;
use sqlx::{PgPool, Row as _};
use uuid::Uuid;

const RULES: &str = include_str!("editor_migration/rules.jsonc");
const GROUPS: &str = include_str!("editor_migration/groups.jsonc");
const DNS: &str = include_str!("editor_migration/dns.jsonc");

pub(crate) async fn run(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let rows = sqlx::query("SELECT id,rule_list,use_system_rule_list,\"group\",use_system_group,filter,use_system_filter,custom_config,use_system_custom_config,dns_config,use_system_dns_config FROM proxy_subscribes WHERE editor_version=0 FOR UPDATE")
        .fetch_all(&mut *tx).await?;
    for row in rows {
        let id: Uuid = row.try_get("id")?;
        let mut values = Vec::new();
        for (field, flag, default, empty_was_default) in [
            ("rule_list", "use_system_rule_list", RULES, true),
            ("group", "use_system_group", GROUPS, true),
            (
                "filter",
                "use_system_filter",
                "[\"官网\",\"客服\",\"qq群\"]",
                false,
            ),
            ("custom_config", "use_system_custom_config", "[]", false),
            ("dns_config", "use_system_dns_config", DNS, false),
        ] {
            let inherited: bool = row.try_get(flag)?;
            let value: Option<String> = row.try_get(field)?;
            values.push(materialize(inherited, value, default, empty_was_default));
        }
        sqlx::query("UPDATE proxy_subscribes SET rule_list=$2,\"group\"=$3,filter=$4,custom_config=$5,dns_config=$6,use_system_rule_list=false,use_system_group=false,use_system_filter=false,use_system_custom_config=false,use_system_dns_config=false,editor_version=1,updated_at=NOW() WHERE id=$1 AND editor_version=0")
            .bind(id).bind(&values[0]).bind(&values[1]).bind(&values[2]).bind(&values[3]).bind(&values[4])
            .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

fn materialize(
    inherited: bool,
    value: Option<String>,
    default: &str,
    empty_was_default: bool,
) -> Option<String> {
    let empty = empty_was_default
        && value.as_deref().is_none_or(|value| {
            value.trim().is_empty()
                || parse_jsonc_value(value).is_ok_and(|value| {
                    value.is_null()
                        || value.as_array().is_some_and(Vec::is_empty)
                        || value.as_object().is_some_and(serde_json::Map::is_empty)
                })
        });
    if inherited || empty {
        Some(default.into())
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freezes_effective_values_and_discards_inactive_values() {
        assert_eq!(
            materialize(true, Some("unused custom value".into()), GROUPS, true).as_deref(),
            Some(GROUPS)
        );
        for empty in ["", "null", "[ ]", "{/* comment */}", "[// comment\n]"] {
            assert_eq!(
                materialize(false, Some(empty.into()), GROUPS, true).as_deref(),
                Some(GROUPS)
            );
        }
        let custom = "[ // retained\n{\"name\":\"custom\",\"type\":\"select\",\"proxies\":[]}]";
        assert_eq!(
            materialize(false, Some(custom.into()), GROUPS, true).as_deref(),
            Some(custom)
        );
        assert_eq!(
            materialize(false, Some("[]".into()), "default", false).as_deref(),
            Some("[]")
        );
        assert_eq!(
            materialize(false, Some("invalid JSONC".into()), GROUPS, true).as_deref(),
            Some("invalid JSONC")
        );
    }
}
