use std::{sync::Arc, time::Duration};

use crate::AppState;

const MAINTENANCE_INTERVAL: Duration = Duration::from_hours(6);

pub(crate) async fn run(state: Arc<AppState>) {
    loop {
        if let Err(error) = cleanup(&state).await {
            tracing::error!(error = %error, "server maintenance failed");
        }
        tokio::time::sleep(MAINTENANCE_INTERVAL).await;
    }
}

async fn cleanup(state: &AppState) -> Result<(), sqlx::Error> {
    let access_logs =
        sqlx::query("DELETE FROM access_logs WHERE created_at < NOW() - ($1 * INTERVAL '1 day')")
            .bind(state.config.access_log_retention_days)
            .execute(&state.pool)
            .await?
            .rows_affected();
    let sessions = sqlx::query("DELETE FROM sessions WHERE expires_at <= NOW()")
        .execute(&state.pool)
        .await?
        .rows_affected();
    let auth_limits = sqlx::query(
        "DELETE FROM auth_limits WHERE updated_at < NOW() - INTERVAL '1 day' AND (blocked_until IS NULL OR blocked_until <= NOW())",
    )
    .execute(&state.pool)
    .await?
    .rows_affected();
    tracing::info!(
        access_logs,
        sessions,
        auth_limits,
        "server maintenance completed"
    );
    Ok(())
}
