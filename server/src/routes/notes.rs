use crate::error::AppError;
use crate::middleware::AuthUser;
use crate::models::{CreateNoteRequest, Note, NoteSummary, UpdateNoteRequest};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use std::sync::Arc;
use uuid::Uuid;

pub async fn create_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<CreateNoteRequest>,
) -> Result<Json<Note>, AppError> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let title = extract_title(&req.content);

    let note: Note = sqlx::query_as(
        "INSERT INTO notes (id, user_id, title, content, is_pinned, created_at, updated_at, version, is_deleted)
         VALUES ($1, $2, $3, $4, false, $5, $5, 1, false)
         RETURNING *",
    )
    .bind(&id)
    .bind(user_id)
    .bind(&title)
    .bind(&req.content)
    .bind(&now)
    .fetch_one(state.db.inner())
    .await?;

    // Record sync change
    record_change(state.as_ref(), user_id, "note", &id, "upsert").await?;

    Ok(Json(note))
}

pub async fn get_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Note>, AppError> {
    let note: Note = sqlx::query_as(
        "SELECT * FROM notes WHERE id = $1 AND user_id = $2",
    )
    .bind(&id)
    .bind(user_id)
    .fetch_optional(state.db.inner())
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(note))
}

pub async fn list_notes(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<Vec<NoteSummary>>, AppError> {
    let notes: Vec<NoteSummary> = sqlx::query_as(
        "SELECT id, title, LEFT(content, 200) as preview, is_pinned, created_at, updated_at
         FROM notes WHERE user_id = $1 AND is_deleted = false
         ORDER BY is_pinned DESC, updated_at DESC",
    )
    .bind(user_id)
    .fetch_all(state.db.inner())
    .await?;

    Ok(Json(notes))
}

pub async fn update_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateNoteRequest>,
) -> Result<Json<Note>, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let title = extract_title(&req.content);

    let note: Note = sqlx::query_as(
        "UPDATE notes SET content = $3, title = $4, updated_at = $5, version = version + 1
         WHERE id = $1 AND user_id = $2 AND is_deleted = false
         RETURNING *",
    )
    .bind(&id)
    .bind(user_id)
    .bind(&req.content)
    .bind(&title)
    .bind(&now)
    .fetch_optional(state.db.inner())
    .await?
    .ok_or(AppError::NotFound)?;

    record_change(state.as_ref(), user_id, "note", &id, "upsert").await?;

    Ok(Json(note))
}

pub async fn delete_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE notes SET is_deleted = true, updated_at = $3 WHERE id = $1 AND user_id = $2",
    )
    .bind(&id)
    .bind(user_id)
    .bind(&now)
    .execute(state.db.inner())
    .await?;

    if result.rows_affected() > 0 {
        record_change(state.as_ref(), user_id, "note", &id, "delete").await?;
    }

    Ok(Json(result.rows_affected() > 0))
}

pub async fn restore_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE notes SET is_deleted = false, updated_at = $3 WHERE id = $1 AND user_id = $2",
    )
    .bind(&id)
    .bind(user_id)
    .bind(&now)
    .execute(state.db.inner())
    .await?;

    if result.rows_affected() > 0 {
        record_change(state.as_ref(), user_id, "note", &id, "upsert").await?;
    }

    Ok(Json(result.rows_affected() > 0))
}

pub async fn toggle_pin(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let note: Option<(bool,)> = sqlx::query_as(
        "UPDATE notes SET is_pinned = NOT is_pinned, updated_at = $3
         WHERE id = $1 AND user_id = $2
         RETURNING is_pinned",
    )
    .bind(&id)
    .bind(user_id)
    .bind(&now)
    .fetch_optional(state.db.inner())
    .await?;

    if let Some((_,)) = note {
        record_change(state.as_ref(), user_id, "note", &id, "upsert").await?;
    }

    Ok(Json(note.is_some()))
}

#[derive(serde::Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

pub async fn search_notes(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<NoteSummary>>, AppError> {
    if params.q.trim().is_empty() {
        return Ok(Json(vec![]));
    }

    let notes: Vec<NoteSummary> = sqlx::query_as(
        "SELECT id, title, LEFT(content, 200) as preview, is_pinned, created_at, updated_at
         FROM notes
         WHERE user_id = $1 AND is_deleted = false
           AND search_vector @@ plainto_tsquery('simple', $2)
         ORDER BY ts_rank(search_vector, plainto_tsquery('simple', $2)) DESC
         LIMIT 50",
    )
    .bind(user_id)
    .bind(&params.q)
    .fetch_all(state.db.inner())
    .await?;

    Ok(Json(notes))
}

fn extract_title(content: &str) -> String {
    // Strip HTML tags and take first line as title
    let text = content
        .replace("<p>", "")
        .replace("</p>", "")
        .replace("<h1>", "")
        .replace("</h1>", "")
        .replace("<br>", "\n")
        .replace("<br/>", "\n");
    let stripped = text
        .split('<')
        .map(|s| s.split('>').last().unwrap_or(""))
        .collect::<String>();
    stripped.lines().next().unwrap_or("Untitled").chars().take(100).collect()
}

async fn record_change(
    state: &AppState,
    user_id: Uuid,
    entity_type: &str,
    entity_id: &str,
    operation: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO cloud_changes (user_id, seq, entity_type, entity_id, operation, envelope)
         VALUES ($1, nextval('cloud_changes_seq'), $2, $3, $4, '{}')",
    )
    .bind(user_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(operation)
    .execute(state.db.inner())
    .await?;

    // Broadcast sync event
    let _ = state.event_tx.send(crate::models::SyncEvent {
        user_id,
        entity_type: entity_type.to_string(),
        entity_id: entity_id.to_string(),
        operation: operation.to_string(),
    });

    Ok(())
}
