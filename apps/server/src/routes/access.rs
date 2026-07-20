use crate::error::AppError;
use crate::AppState;
use chrono::Utc;
use uuid::Uuid;

const MAX_ATTACHMENT_BYTES: i64 = 2 * 1024 * 1024 * 1024;
const MAX_ACTIVE_DEVICES: i64 = 50;
const ACTIVE_DEVICE_WINDOW_DAYS: i64 = 45;
const VERSION_HISTORY_DAYS: i64 = 30;

pub async fn ensure_attachment_quota(
    state: &AppState,
    user_id: Uuid,
    incoming_size: i64,
) -> Result<(), AppError> {
    ensure_user_exists(state, user_id).await?;
    let used: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size),0)::BIGINT FROM attachments WHERE user_id=$1",
    )
    .bind(user_id)
    .fetch_one(state.db.inner())
    .await?;
    if used + incoming_size > MAX_ATTACHMENT_BYTES {
        return Err(AppError::BadRequest(format!(
            "Attachment storage limit exceeded. Used {used} bytes of {MAX_ATTACHMENT_BYTES} bytes."
        )));
    }
    Ok(())
}

pub async fn ensure_device_allowed(
    state: &AppState,
    user_id: Uuid,
    device_id: &str,
) -> Result<(), AppError> {
    ensure_user_exists(state, user_id).await?;
    let known: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sync_cursors WHERE user_id=$1 AND device_id=$2)",
    )
    .bind(user_id)
    .bind(device_id)
    .fetch_one(state.db.inner())
    .await?;
    if known {
        return Ok(());
    }
    let cutoff = Utc::now() - chrono::Duration::days(ACTIVE_DEVICE_WINDOW_DAYS);
    let active: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sync_cursors WHERE user_id=$1 AND updated_at>=$2")
            .bind(user_id)
            .bind(cutoff)
            .fetch_one(state.db.inner())
            .await?;
    if active >= MAX_ACTIVE_DEVICES {
        return Err(AppError::BadRequest(
            "Active device safety limit exceeded".into(),
        ));
    }
    Ok(())
}

pub async fn version_history_cutoff(
    state: &AppState,
    user_id: Uuid,
) -> Result<Option<String>, AppError> {
    ensure_user_exists(state, user_id).await?;
    Ok(Some(
        (Utc::now() - chrono::Duration::days(VERSION_HISTORY_DAYS))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    ))
}

async fn ensure_user_exists(state: &AppState, user_id: Uuid) -> Result<(), AppError> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id=$1)")
        .bind(user_id)
        .fetch_one(state.db.inner())
        .await?;
    if exists {
        Ok(())
    } else {
        Err(AppError::Auth)
    }
}
