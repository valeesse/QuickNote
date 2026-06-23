use crate::error::AppError;
use crate::middleware::AuthUser;
use crate::models::{CaptureClipboardRequest, ClipboardItem};
use crate::routes::sync::{append_change, ChangePayload};
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use sha2::{Digest, Sha256};
use std::sync::Arc;

const COLUMNS: &str = "id,kind,content,preview,source_device,created_at,updated_at,last_copied_at,capture_count,is_pinned,is_deleted";

pub async fn list_items(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<Vec<ClipboardItem>>, AppError> {
    let query = format!("SELECT {COLUMNS} FROM clipboard_items WHERE user_id=$1 AND is_deleted=false ORDER BY is_pinned DESC,last_copied_at DESC LIMIT 300");
    Ok(Json(
        sqlx::query_as(&query)
            .bind(user_id)
            .fetch_all(state.db.inner())
            .await?,
    ))
}

pub async fn capture(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<CaptureClipboardRequest>,
) -> Result<Json<ClipboardItem>, AppError> {
    if req.content.trim().is_empty() {
        return Err(AppError::BadRequest("Content cannot be empty".into()));
    }
    if req.content.len() > 2 * 1024 * 1024 {
        return Err(AppError::BadRequest(
            "Clipboard content exceeds 2 MB".into(),
        ));
    }
    let id = format!("{:x}", Sha256::digest(req.content.as_bytes()));
    let kind = req.kind.unwrap_or_else(|| detect_kind(&req.content));
    let device = req.source_device.unwrap_or_else(|| "web".into());
    let now = chrono::Utc::now().to_rfc3339();
    let preview: String = req.content.chars().take(200).collect();
    let mut tx = state.db.inner().begin().await?;
    let query = format!("INSERT INTO clipboard_items (id,user_id,kind,content,preview,source_device,created_at,updated_at,last_copied_at,capture_count,is_pinned,is_deleted) VALUES ($1,$2,$3,$4,$5,$6,$7,$7,$7,1,false,false) ON CONFLICT (user_id,id) DO UPDATE SET capture_count=clipboard_items.capture_count+1,last_copied_at=EXCLUDED.last_copied_at,updated_at=EXCLUDED.updated_at,is_deleted=false RETURNING {COLUMNS}");
    let item: ClipboardItem = sqlx::query_as(&query)
        .bind(&id)
        .bind(user_id)
        .bind(kind)
        .bind(req.content)
        .bind(preview)
        .bind(device)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
    append_change(
        &mut tx,
        user_id,
        "clipboard",
        &id,
        "upsert",
        ChangePayload::Clipboard(item.clone()),
    )
    .await?;
    tx.commit().await?;
    notify(&state, user_id, &id, "upsert");
    Ok(Json(item))
}

pub async fn delete_item(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let mut tx = state.db.inner().begin().await?;
    let query = format!("UPDATE clipboard_items SET is_deleted=true,updated_at=$3 WHERE id=$1 AND user_id=$2 AND is_deleted=false RETURNING {COLUMNS}");
    let item: Option<ClipboardItem> = sqlx::query_as(&query)
        .bind(&id)
        .bind(user_id)
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_optional(&mut *tx)
        .await?;
    let Some(item) = item else {
        tx.rollback().await?;
        return Ok(Json(false));
    };
    append_change(
        &mut tx,
        user_id,
        "clipboard",
        &id,
        "delete",
        ChangePayload::Clipboard(item),
    )
    .await?;
    tx.commit().await?;
    notify(&state, user_id, &id, "delete");
    Ok(Json(true))
}

fn notify(state: &AppState, user_id: uuid::Uuid, id: &str, operation: &str) {
    let _ = state.event_tx.send(crate::models::SyncEvent {
        user_id,
        entity_type: "clipboard".into(),
        entity_id: id.into(),
        operation: operation.into(),
    });
}

fn detect_kind(content: &str) -> String {
    if content.starts_with("http://") || content.starts_with("https://") {
        "link".into()
    } else if content.contains("fn ") || content.contains("function ") || content.contains("=>") {
        "code".into()
    } else {
        "text".into()
    }
}
