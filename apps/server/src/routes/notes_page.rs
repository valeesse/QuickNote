use crate::error::AppError;
use crate::middleware::AuthUser;
use crate::models::NoteSummary;
use crate::AppState;
use axum::extract::{Query, State};
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct PageQuery {
    cursor: Option<String>,
    tag: Option<String>,
    limit: Option<i64>,
}

#[derive(Serialize)]
pub struct NotePage {
    items: Vec<NoteSummary>,
    next_cursor: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct NoteCursor {
    pinned: i32,
    sort_order: i64,
    updated_at: String,
    id: String,
}

#[derive(sqlx::FromRow)]
struct PageRow {
    id: String,
    title: String,
    preview: String,
    is_pinned: bool,
    sort_order: i64,
    created_at: String,
    updated_at: String,
    tags: Vec<String>,
}

pub async fn list_page(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Query(query): Query<PageQuery>,
) -> Result<Json<NotePage>, AppError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let cursor = query.cursor.as_deref().map(decode_cursor).transpose()?;
    let normalized_tag = query
        .tag
        .map(|tag| tag.trim().to_lowercase())
        .filter(|tag| !tag.is_empty());
    let pinned = cursor.as_ref().map(|value| value.pinned);
    let sort_order = cursor.as_ref().map(|value| value.sort_order);
    let updated_at = cursor.as_ref().map(|value| value.updated_at.as_str());
    let id = cursor.as_ref().map(|value| value.id.as_str());
    let rows: Vec<PageRow> = sqlx::query_as(
        "SELECT n.id,n.title,LEFT(n.content,200) AS preview,n.is_pinned,n.sort_order,
         n.created_at,n.updated_at,COALESCE(array_agg(t.name ORDER BY lower(t.name))
         FILTER (WHERE t.id IS NOT NULL),ARRAY[]::TEXT[]) AS tags
         FROM notes n
         LEFT JOIN note_tags nt ON nt.user_id=n.user_id AND nt.note_id=n.id
         LEFT JOIN tags t ON t.user_id=n.user_id AND t.id=nt.tag_id AND t.is_deleted=false
         WHERE n.user_id=$1 AND n.is_deleted=false
         AND ($2::TEXT IS NULL OR EXISTS(SELECT 1 FROM note_tags fnt JOIN tags ft
           ON ft.user_id=fnt.user_id AND ft.id=fnt.tag_id WHERE fnt.user_id=n.user_id
           AND fnt.note_id=n.id AND ft.is_deleted=false AND ft.normalized_name=$2))
         AND ($3::INT IS NULL OR CASE WHEN n.is_pinned THEN 1 ELSE 0 END < $3
           OR (CASE WHEN n.is_pinned THEN 1 ELSE 0 END=$3 AND n.sort_order<$4)
           OR (CASE WHEN n.is_pinned THEN 1 ELSE 0 END=$3 AND n.sort_order=$4 AND n.updated_at<$5)
           OR (CASE WHEN n.is_pinned THEN 1 ELSE 0 END=$3 AND n.sort_order=$4
               AND n.updated_at=$5 AND n.id<$6))
         GROUP BY n.id,n.title,n.content,n.is_pinned,n.sort_order,n.created_at,n.updated_at
         ORDER BY n.is_pinned DESC,n.sort_order DESC,n.updated_at DESC,n.id DESC LIMIT $7",
    )
    .bind(user_id)
    .bind(normalized_tag)
    .bind(pinned)
    .bind(sort_order)
    .bind(updated_at)
    .bind(id)
    .bind(limit + 1)
    .fetch_all(state.db.inner())
    .await?;
    let has_more = rows.len() > limit as usize;
    let rows = rows.into_iter().take(limit as usize).collect::<Vec<_>>();
    let next_cursor = if has_more {
        rows.last().map(encode_cursor).transpose()?
    } else {
        None
    };
    Ok(Json(NotePage {
        items: rows.into_iter().map(summary).collect(),
        next_cursor,
    }))
}

fn summary(row: PageRow) -> NoteSummary {
    NoteSummary {
        id: row.id,
        title: row.title,
        preview: row.preview,
        is_pinned: row.is_pinned,
        created_at: row.created_at,
        updated_at: row.updated_at,
        tags: row.tags,
    }
}

fn encode_cursor(row: &PageRow) -> Result<String, AppError> {
    let value = NoteCursor {
        pinned: i32::from(row.is_pinned),
        sort_order: row.sort_order,
        updated_at: row.updated_at.clone(),
        id: row.id.clone(),
    };
    serde_json::to_vec(&value)
        .map(|bytes| URL_SAFE_NO_PAD.encode(bytes))
        .map_err(|error| AppError::Internal(error.to_string()))
}

fn decode_cursor(value: &str) -> Result<NoteCursor, AppError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AppError::BadRequest("Invalid note cursor".into()))?;
    serde_json::from_slice(&bytes).map_err(|_| AppError::BadRequest("Invalid note cursor".into()))
}
