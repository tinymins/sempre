use sqlx::Row as _;
use uuid::Uuid;

use crate::error::ApiError;

pub(crate) async fn sync_selected(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    selected: &[Uuid],
) -> Result<(), ApiError> {
    let assigned = sqlx::query(
        "SELECT custom_node_id FROM proxy_subscribe_custom_nodes WHERE subscribe_id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_all(&mut **transaction)
    .await?;
    let assigned = assigned
        .iter()
        .map(|row| {
            row.try_get::<Uuid, _>("custom_node_id")
                .map_err(ApiError::internal)
        })
        .collect::<Result<std::collections::HashSet<_>, _>>()?;
    if !selected.iter().all(|node_id| assigned.contains(node_id)) {
        return Err(ApiError::bad_request(
            "only assigned custom nodes can be enabled",
        ));
    }
    sqlx::query("UPDATE proxy_subscribe_custom_nodes SET enabled=FALSE WHERE subscribe_id=$1")
        .bind(id)
        .execute(&mut **transaction)
        .await?;
    for (position, node_id) in selected.iter().enumerate() {
        sqlx::query("UPDATE proxy_subscribe_custom_nodes SET enabled=TRUE,position=$1,updated_at=NOW() WHERE subscribe_id=$2 AND custom_node_id=$3")
            .bind(i32::try_from(position).map_err(ApiError::internal)?).bind(id).bind(node_id)
            .execute(&mut **transaction).await?;
    }
    Ok(())
}
