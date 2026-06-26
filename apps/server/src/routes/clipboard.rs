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
    let query = format!("SELECT {COLUMNS} FROM clipboard_items WHERE user_id=$1 AND is_deleted=false ORDER BY is_pinned DESC,last_copied_at DESC,created_at DESC LIMIT 300");
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
    if req.content.len() > 5 * 1024 * 1024 {
        return Err(AppError::BadRequest(
            "Clipboard content exceeds 5 MB".into(),
        ));
    }
    let normalized = normalize_clipboard_content(&req.content);
    let id = format!(
        "{:x}",
        Sha256::digest(format!("clipboard:text:{normalized}"))
    );
    let kind = req.kind.unwrap_or_else(|| detect_kind(&normalized));
    let device = req.source_device.unwrap_or_else(|| "web".into());
    let now = chrono::Utc::now().to_rfc3339();
    let preview = clipboard_preview(&normalized, &kind);
    let mut tx = state.db.inner().begin().await?;
    let query = format!(
        "INSERT INTO clipboard_items
            (id,user_id,kind,content,preview,source_device,created_at,updated_at,last_copied_at,capture_count,is_pinned,is_deleted,created_by,updated_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$7,$7,1,false,false,$2,$2)
         ON CONFLICT (user_id,id) DO UPDATE SET
            capture_count=clipboard_items.capture_count+1,
            last_copied_at=EXCLUDED.last_copied_at,
            updated_at=EXCLUDED.updated_at,
            is_deleted=false,
            updated_by=EXCLUDED.updated_by
         RETURNING {COLUMNS}"
    );
    let item: ClipboardItem = sqlx::query_as(&query)
        .bind(&id)
        .bind(user_id)
        .bind(kind)
        .bind(normalized)
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
    let item: Option<ClipboardItem> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM clipboard_items WHERE id=$1 AND user_id=$2 AND is_deleted=false"
    ))
    .bind(&id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(item) = item else {
        tx.rollback().await?;
        return Ok(Json(false));
    };
    sqlx::query("DELETE FROM clipboard_items WHERE id=$1 AND user_id=$2")
        .bind(&id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
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

pub async fn toggle_pin(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let mut tx = state.db.inner().begin().await?;
    let query = format!(
        "UPDATE clipboard_items
         SET is_pinned=NOT is_pinned,updated_at=$3,updated_by=$2
         WHERE id=$1 AND user_id=$2 AND is_deleted=false
         RETURNING {COLUMNS}"
    );
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
        "upsert",
        ChangePayload::Clipboard(item),
    )
    .await?;
    tx.commit().await?;
    notify(&state, user_id, &id, "upsert");
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
    if content.starts_with("data:image/") || content.starts_with("<img ") {
        "image".into()
    } else if looks_like_rich_clipboard(content) {
        "rich".into()
    } else if content.starts_with("http://") || content.starts_with("https://") {
        "link".into()
    } else if content.contains("fn ") || content.contains("function ") || content.contains("=>") {
        "code".into()
    } else {
        "text".into()
    }
}

fn looks_like_rich_clipboard(content: &str) -> bool {
    let lowered = content.to_ascii_lowercase();
    lowered.contains("<img ")
        || lowered.contains("<table")
        || lowered.contains("<ul")
        || lowered.contains("<ol")
        || lowered.contains("<li")
        || lowered.contains("<pre")
        || lowered.contains("<code")
        || lowered.contains("<blockquote")
        || lowered.contains("<figure")
        || lowered.contains("<br")
        || count_html_block_tags(&lowered) > 1
}

fn count_html_block_tags(lowered: &str) -> usize {
    [
        "<p", "<div", "<section", "<article", "<h1", "<h2", "<h3", "<h4", "<h5", "<h6",
    ]
    .iter()
    .map(|needle| lowered.matches(needle).count())
    .sum()
}

fn clipboard_preview(content: &str, kind: &str) -> String {
    if kind == "image" {
        return "图片".into();
    }
    let preview = if kind == "rich" {
        strip_html_tags(content)
    } else {
        content.replace('\n', " ")
    };
    preview.chars().take(200).collect()
}

fn normalize_clipboard_content(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

fn strip_html_tags(content: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let mut in_tag = false;
    for ch in content.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                output.push(' ');
            }
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}
