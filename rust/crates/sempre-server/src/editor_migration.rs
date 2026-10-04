//! Mark existing subscriptions as compatible with the shared editor without changing their settings.
use sqlx::PgPool;

pub(crate) async fn run(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE proxy_subscribes SET editor_version=1 WHERE editor_version=0")
        .execute(pool)
        .await?;
    Ok(())
}
