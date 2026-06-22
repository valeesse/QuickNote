use crate::error::AppError;
use crate::middleware::AuthUser;
use crate::models::{CaptureClipboardRequest, ClipboardItem};
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub async fn list_items(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<Vec<ClipboardItem>>, AppError> {
    let items: Vec<ClipboardItem> = sqlx::query_as(
        "SELECT * FROM clipboard_items WHERE user_id = $1 AND is_deleted = false
         ORDER BY is_pinned DESC, last_copied_at DESC LIMIT 300",
    )
    .bind(user_id)
    .fetch_all(state.db.inner())
    .await?;

    Ok(Json(items))
}

pub async fn capture(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<CaptureClipboardRequest>,
) -> Result<Json<ClipboardItem>, AppError> {
    if req.content.trim().is_empty() {
        return Err(AppError::BadRequest("Content cannot be empty".into()));
    }

    let id = format!("{:x}", Sha256::digest(req.content.as_bytes()));
    let kind = req.kind.unwrap_or_else(|| detect_kind(&req.content));
    let device = req.source_device.unwrap_or_else(|| "web".to_string());
    let now = chrono::Utc::now().to_rfc3339();
    let preview = req.content.chars().take(200).collect::<String>();

    // Upsert: if already exists for this user, increment count
    let existing: Option<ClipboardItem> = sqlx::query_as(
        "SELECT * FROM clipboard_items WHERE id = $1 AND user_id = $2",
    )
    .bind(&id)
    .bind(user_id)
    .fetch_optional(state.db.inner())
    .await?;

    if existing.is_some() {
        let item: ClipboardItem = sqlx::query_as(
            "UPDATE clipboard_items SET capture_count = capture_count + 1, last_copied_at = $3, updated_at = $3
             WHERE id = $1 AND user_id = $2 RETURNING *",
        )
        .bind(&id)
        .bind(user_id)
        .bind(&now)
        .fetch_one(state.db.inner())
        .await?;
        return Ok(Json(item));
    }

    let item: ClipboardItem = sqlx::query_as(
        "INSERT INTO clipboard_items (id, user_id, kind, content, preview, source_device, created_at, updated_at, last_copied_at, capture_count, is_pinned, is_deleted)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $7, 1, false, false)
         RETURNING *",
    )
    .bind(&id)
    .bind(user_id)
    .bind(&kind)
    .bind(&req.content)
    .bind(&preview)
    .bind(&device)
    .bind(&now)
    .fetch_one(state.db.inner())
    .await?;

    // Record sync change
    sqlx::query(
        "INSERT INTO cloud_changes (user_id, seq, entity_type, entity_id, operation, envelope)
         VALUES ($1, nextval('cloud_changes_seq'), 'clipboard', $2, 'upsert', '{}')",
    )
    .bind(user_id)
    .bind(&id)
    .execute(state.db.inner())
    .await?;

    Ok(Json(item))
}

pub async fn delete_item(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let result = sqlx::query(
        "UPDATE clipboard_items SET is_deleted = true WHERE id = $1 AND user_id = $2",
    )
    .bind(&id)
    .bind(user_id)
    .execute(state.db.inner())
    .await?;

    Ok(Json(result.rows_affected() > 0))
}

fn detect_kind(content: &str) -> String {
    if content.starts_with("http://") || content.starts_with("https://") {
        "link".to_string()
    } else if content.contains("fn ") || content.contains("function ") || content.contains("=>") {
        "code".to_string()
    } else {
        "text".to_string()
    }
}
