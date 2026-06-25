use crate::error::AppError;
use crate::middleware::AuthUser;
use crate::models::{
    CreateNoteRequest, Note, NoteSummary, NoteVersion, ReorderNotesRequest, UpdateNoteRequest,
};
use crate::routes::attachments::delete_attachment_object;
use crate::routes::billing::version_history_cutoff;
use crate::routes::sync::{append_change, ChangePayload};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use regex::Regex;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::OnceLock;
use uuid::Uuid;

const NOTE_COLUMNS: &str =
    "id,title,content,yjs_state,yjs_state_version,is_pinned,sort_order,created_at,updated_at,version,is_deleted";

pub async fn create_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<CreateNoteRequest>,
) -> Result<Json<Note>, AppError> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let title = extract_title(&req.content);
    let mut tx = state.db.inner().begin().await?;
    let sort_order: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM notes WHERE user_id=$1 AND is_deleted=false AND is_pinned=false",
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    let query = format!(
        "INSERT INTO notes (id,user_id,title,content,is_pinned,sort_order,created_at,updated_at,version,is_deleted,created_by,updated_by)
         VALUES ($1,$2,$3,$4,false,$5,$6,$6,1,false,$2,$2)
         RETURNING {NOTE_COLUMNS}"
    );
    let note: Note = sqlx::query_as(&query)
        .bind(&id)
        .bind(user_id)
        .bind(title)
        .bind(req.content)
        .bind(sort_order)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
    append_change(
        &mut tx,
        user_id,
        "note",
        &id,
        "upsert",
        ChangePayload::Note(note.clone()),
    )
    .await?;
    tx.commit().await?;
    notify(&state, user_id, &id, "upsert");
    Ok(Json(note))
}

pub async fn get_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Note>, AppError> {
    let query = format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id=$1 AND user_id=$2");
    let note = sqlx::query_as(&query)
        .bind(id)
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
    let notes = sqlx::query_as("SELECT id,title,LEFT(content,200) AS preview,is_pinned,created_at,updated_at FROM notes WHERE user_id=$1 AND is_deleted=false ORDER BY is_pinned DESC,sort_order DESC,updated_at DESC")
        .bind(user_id).fetch_all(state.db.inner()).await?;
    Ok(Json(notes))
}

pub async fn reorder_notes(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<ReorderNotesRequest>,
) -> Result<Json<bool>, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = state.db.inner().begin().await?;
    let len = req.ids.len() as i64;
    for (index, id) in req.ids.iter().enumerate() {
        let query = format!(
            "UPDATE notes
             SET is_pinned=$3,sort_order=$4,updated_at=$5,updated_by=$2
             WHERE id=$1 AND user_id=$2 AND is_deleted=false
             RETURNING {NOTE_COLUMNS}"
        );
        let note: Option<Note> = sqlx::query_as(&query)
            .bind(id)
            .bind(user_id)
            .bind(req.is_pinned)
            .bind(len - index as i64)
            .bind(&now)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(note) = note {
            append_change(
                &mut tx,
                user_id,
                "note",
                id,
                "upsert",
                ChangePayload::Note(note),
            )
            .await?;
            notify(&state, user_id, id, "upsert");
        }
    }
    tx.commit().await?;
    Ok(Json(true))
}

pub async fn update_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateNoteRequest>,
) -> Result<Json<Note>, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let title = extract_title(&req.content);
    let version_cutoff = version_history_cutoff(&state, user_id).await?;
    let mut tx = state.db.inner().begin().await?;

    // Snapshot current version before updating, then prune by plan policy.
    let existing: Option<Note> = sqlx::query_as(&format!(
        "SELECT {NOTE_COLUMNS} FROM notes WHERE id=$1 AND user_id=$2 AND is_deleted=false"
    ))
    .bind(&id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(old) = existing {
        sqlx::query(
            "INSERT INTO note_versions
                (note_id,user_id,title,content,version,created_at,updated_at,is_pinned,created_by,updated_by)
             VALUES ($1,$2,$3,$4,$5,$6,$6,false,$2,$2)",
        )
            .bind(&id)
            .bind(user_id)
            .bind(&old.title)
            .bind(&old.content)
            .bind(old.version)
            .bind(&old.updated_at)
            .execute(&mut *tx)
            .await?;
        if let Some(cutoff) = version_cutoff.as_deref() {
            sqlx::query(
                "DELETE FROM note_versions
                 WHERE user_id=$1 AND is_pinned=false AND created_at < $2",
            )
            .bind(user_id)
            .bind(cutoff)
            .execute(&mut *tx)
            .await?;
        }
    }

    let query = format!(
        "UPDATE notes
         SET content=$3,title=$4,updated_at=$5,version=version+1,updated_by=$2
         WHERE id=$1 AND user_id=$2 AND is_deleted=false
         RETURNING {NOTE_COLUMNS}"
    );
    let note: Note = sqlx::query_as(&query)
        .bind(&id)
        .bind(user_id)
        .bind(req.content)
        .bind(title)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    append_change(
        &mut tx,
        user_id,
        "note",
        &id,
        "upsert",
        ChangePayload::Note(note.clone()),
    )
    .await?;
    tx.commit().await?;
    notify(&state, user_id, &id, "upsert");
    Ok(Json(note))
}

pub async fn prune_versions_to_plan_window(
    state: &AppState,
    user_id: Uuid,
) -> Result<(), AppError> {
    let Some(cutoff) = version_history_cutoff(state, user_id).await? else {
        return Ok(());
    };
    sqlx::query(
        "DELETE FROM note_versions
         WHERE user_id=$1 AND is_pinned=false AND created_at < $2",
    )
    .bind(user_id)
    .bind(cutoff)
    .execute(state.db.inner())
    .await?;
    Ok(())
}

pub async fn delete_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    mutate_flag(
        &state,
        user_id,
        &id,
        "UPDATE notes SET is_deleted=true,updated_at=$3,updated_by=$4 WHERE id=$1 AND user_id=$2 AND is_deleted=false",
        "delete",
    )
    .await
}

pub async fn restore_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    mutate_flag(
        &state,
        user_id,
        &id,
        "UPDATE notes SET is_deleted=false,updated_at=$3,updated_by=$4 WHERE id=$1 AND user_id=$2 AND is_deleted=true",
        "upsert",
    )
    .await
}

pub async fn toggle_pin(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let mut tx = state.db.inner().begin().await?;
    let now = chrono::Utc::now().to_rfc3339();
    // Read current pin state to compute target sort_order
    let current: Option<(bool, i64)> = sqlx::query_as(
        "SELECT is_pinned, sort_order FROM notes WHERE id=$1 AND user_id=$2 AND is_deleted=false",
    )
    .bind(&id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((is_pinned, _)) = current {
        let target_pinned = !is_pinned;
        let next_sort_order: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM notes WHERE user_id=$1 AND is_deleted=false AND is_pinned=$2",
        )
        .bind(user_id)
        .bind(target_pinned)
        .fetch_one(&mut *tx)
        .await?;
        let query = format!(
            "UPDATE notes
             SET is_pinned=$3,sort_order=$4,updated_at=$5,updated_by=$2
             WHERE id=$1 AND user_id=$2 AND is_deleted=false
             RETURNING {NOTE_COLUMNS}"
        );
        let note: Option<Note> = sqlx::query_as(&query)
            .bind(&id)
            .bind(user_id)
            .bind(target_pinned)
            .bind(next_sort_order)
            .bind(&now)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(note) = note {
            append_change(
                &mut tx,
                user_id,
                "note",
                &id,
                "upsert",
                ChangePayload::Note(note),
            )
            .await?;
            tx.commit().await?;
            notify(&state, user_id, &id, "upsert");
            return Ok(Json(true));
        }
    }
    tx.rollback().await?;
    Ok(Json(false))
}

async fn mutate_flag(
    state: &Arc<AppState>,
    user_id: Uuid,
    id: &str,
    sql: &str,
    operation: &str,
) -> Result<Json<bool>, AppError> {
    let mut tx = state.db.inner().begin().await?;
    let result = sqlx::query(sql)
        .bind(id)
        .bind(user_id)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(Json(false));
    }
    let note = if operation == "upsert" {
        let query = format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id=$1 AND user_id=$2");
        Some(
            sqlx::query_as(&query)
                .bind(id)
                .bind(user_id)
                .fetch_one(&mut *tx)
                .await?,
        )
    } else {
        None
    };
    let payload = note.map(ChangePayload::Note).unwrap_or(ChangePayload::None);
    append_change(&mut tx, user_id, "note", id, operation, payload).await?;
    tx.commit().await?;
    notify(state, user_id, id, operation);
    Ok(Json(true))
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
    let notes = sqlx::query_as("SELECT id,title,LEFT(content,200) AS preview,is_pinned,created_at,updated_at FROM notes WHERE user_id=$1 AND is_deleted=false AND search_vector @@ plainto_tsquery('simple',$2) ORDER BY ts_rank(search_vector,plainto_tsquery('simple',$2)) DESC LIMIT 50")
        .bind(user_id).bind(params.q).fetch_all(state.db.inner()).await?;
    Ok(Json(notes))
}

pub async fn list_deleted_notes(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<Vec<NoteSummary>>, AppError> {
    let notes = sqlx::query_as(
        "SELECT id,title,LEFT(content,200) AS preview,is_pinned,created_at,updated_at FROM notes WHERE user_id=$1 AND is_deleted=true ORDER BY updated_at DESC",
    )
    .bind(user_id)
    .fetch_all(state.db.inner())
    .await?;
    Ok(Json(notes))
}

pub async fn purge_note(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let mut tx = state.db.inner().begin().await?;
    let note_content: Option<String> = sqlx::query_scalar(
        "SELECT content FROM notes WHERE id=$1 AND user_id=$2 AND is_deleted=true",
    )
    .bind(&id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(note_content) = note_content else {
        tx.rollback().await?;
        return Ok(Json(false));
    };
    let version_contents: Vec<String> =
        sqlx::query_scalar("SELECT content FROM note_versions WHERE note_id=$1 AND user_id=$2")
            .bind(&id)
            .bind(user_id)
            .fetch_all(&mut *tx)
            .await?;
    let candidate_attachment_ids = collect_attachment_candidates(
        std::iter::once(note_content.as_str()).chain(version_contents.iter().map(String::as_str)),
    );
    let mut orphaned_attachments = Vec::new();
    for attachment_id in candidate_attachment_ids {
        if !attachment_is_still_referenced(&mut tx, user_id, &attachment_id, &id).await? {
            orphaned_attachments.push(attachment_id);
        }
    }

    sqlx::query("DELETE FROM note_versions WHERE note_id=$1 AND user_id=$2")
        .bind(&id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    for attachment_id in &orphaned_attachments {
        sqlx::query("DELETE FROM attachments WHERE user_id=$1 AND id=$2")
            .bind(user_id)
            .bind(attachment_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("DELETE FROM notes WHERE id=$1 AND user_id=$2")
        .bind(&id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    for attachment_id in &orphaned_attachments {
        if let Err(error) = delete_attachment_object(&state, user_id, attachment_id).await {
            tracing::warn!(
                user_id = %user_id,
                attachment_id,
                error = %error,
                "attachment object cleanup failed after note purge"
            );
        }
    }
    notify(&state, user_id, &id, "delete");
    Ok(Json(true))
}

pub async fn list_versions(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<NoteVersion>>, AppError> {
    let versions = if let Some(cutoff) = version_history_cutoff(&state, user_id).await? {
        sqlx::query_as(
            "SELECT id,note_id,title,content,version,created_at,is_pinned
             FROM note_versions
             WHERE note_id=$1 AND user_id=$2 AND created_at >= $3
             ORDER BY created_at DESC",
        )
        .bind(&id)
        .bind(user_id)
        .bind(cutoff)
        .fetch_all(state.db.inner())
        .await?
    } else {
        sqlx::query_as(
            "SELECT id,note_id,title,content,version,created_at,is_pinned
             FROM note_versions
             WHERE note_id=$1 AND user_id=$2
             ORDER BY created_at DESC",
        )
        .bind(&id)
        .bind(user_id)
        .fetch_all(state.db.inner())
        .await?
    };
    Ok(Json(versions))
}

pub async fn restore_version(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path((note_id, version_id)): Path<(String, i64)>,
) -> Result<Json<Note>, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = state.db.inner().begin().await?;

    // Fetch the version content
    let ver: Option<NoteVersion> = sqlx::query_as(
        "SELECT id,note_id,title,content,version,created_at,is_pinned FROM note_versions WHERE id=$1 AND note_id=$2 AND user_id=$3",
    )
    .bind(version_id)
    .bind(&note_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let ver = ver.ok_or(AppError::NotFound)?;
    if let Some(cutoff) = version_history_cutoff(&state, user_id).await? {
        if ver.created_at < cutoff {
            return Err(AppError::BadRequest(
                "This version is outside the current plan's history window.".into(),
            ));
        }
    }

    // Update the note with version content
    let query = format!(
        "UPDATE notes
         SET content=$3,title=$4,updated_at=$5,version=version+1,updated_by=$2
         WHERE id=$1 AND user_id=$2 AND is_deleted=false
         RETURNING {NOTE_COLUMNS}"
    );
    let note: Note = sqlx::query_as(&query)
        .bind(&note_id)
        .bind(user_id)
        .bind(&ver.content)
        .bind(&ver.title)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;

    append_change(
        &mut tx,
        user_id,
        "note",
        &note_id,
        "upsert",
        ChangePayload::Note(note.clone()),
    )
    .await?;
    tx.commit().await?;
    notify(&state, user_id, &note_id, "upsert");
    Ok(Json(note))
}

pub async fn toggle_version_pin(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(version_id): Path<i64>,
) -> Result<Json<bool>, AppError> {
    let result = sqlx::query(
        "UPDATE note_versions
             SET is_pinned=NOT is_pinned, updated_at=$3, updated_by=$2
             WHERE id=$1 AND user_id=$2",
    )
    .bind(version_id)
    .bind(user_id)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(state.db.inner())
    .await?;
    Ok(Json(result.rows_affected() > 0))
}

pub async fn delete_version(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(version_id): Path<i64>,
) -> Result<Json<bool>, AppError> {
    let result = sqlx::query("DELETE FROM note_versions WHERE id=$1 AND user_id=$2")
        .bind(version_id)
        .bind(user_id)
        .execute(state.db.inner())
        .await?;
    Ok(Json(result.rows_affected() > 0))
}

pub async fn clear_versions(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Path(note_id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let result = sqlx::query(
        "DELETE FROM note_versions WHERE note_id=$1 AND user_id=$2 AND is_pinned=false",
    )
    .bind(&note_id)
    .bind(user_id)
    .execute(state.db.inner())
    .await?;
    Ok(Json(result.rows_affected() > 0))
}

fn extract_title(content: &str) -> String {
    let mut plain = String::with_capacity(content.len());
    let mut in_tag = false;
    let mut tag = String::new();
    for ch in content.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' => {
                in_tag = false;
                let tag = tag.trim().to_ascii_lowercase();
                if tag.starts_with("br")
                    || tag.starts_with("/p")
                    || tag.starts_with("/h")
                    || tag.starts_with("/li")
                    || tag.starts_with("/div")
                    || tag.starts_with("/pre")
                {
                    plain.push('\n');
                } else {
                    plain.push(' ');
                }
            }
            _ if in_tag => tag.push(ch),
            _ if !in_tag => plain.push(ch),
            _ => {}
        }
    }
    let title = plain
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim_start_matches('#')
        .trim();
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let title: String = title.chars().take(100).collect();
    if title.is_empty() {
        "Untitled".to_string()
    } else {
        title
    }
}

fn notify(state: &AppState, user_id: Uuid, id: &str, operation: &str) {
    let _ = state.event_tx.send(crate::models::SyncEvent {
        user_id,
        entity_type: "note".into(),
        entity_id: id.into(),
        operation: operation.into(),
    });
}

fn attachment_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"attachment://([a-f0-9]{64})").expect("valid attachment regex"))
}

fn collect_attachment_candidates<'a>(contents: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for content in contents {
        for captures in attachment_regex().captures_iter(content) {
            if let Some(id) = captures.get(1) {
                ids.insert(id.as_str().to_string());
            }
        }
    }
    ids.into_iter().collect()
}

async fn attachment_is_still_referenced(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    attachment_id: &str,
    purged_note_id: &str,
) -> Result<bool, AppError> {
    let pattern = format!("%attachment://{attachment_id}%");
    let referenced_in_notes: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM notes
            WHERE user_id=$1 AND id<>$2 AND content LIKE $3
        )",
    )
    .bind(user_id)
    .bind(purged_note_id)
    .bind(&pattern)
    .fetch_one(&mut **tx)
    .await?;
    if referenced_in_notes {
        return Ok(true);
    }
    let referenced_in_versions: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM note_versions
            WHERE user_id=$1 AND note_id<>$2 AND content LIKE $3
        )",
    )
    .bind(user_id)
    .bind(purged_note_id)
    .bind(&pattern)
    .fetch_one(&mut **tx)
    .await?;
    if referenced_in_versions {
        return Ok(true);
    }
    let referenced_in_clipboard: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM clipboard_items
            WHERE user_id=$1 AND is_deleted=false AND content LIKE $2
        )",
    )
    .bind(user_id)
    .bind(&pattern)
    .fetch_one(&mut **tx)
    .await?;
    Ok(referenced_in_clipboard)
}

#[cfg(test)]
mod tests {
    use super::{collect_attachment_candidates, extract_title};

    #[test]
    fn title_is_plain_text_and_bounded() {
        assert_eq!(
            extract_title("<h1>Hello <strong>cloud</strong></h1>"),
            "Hello cloud"
        );
        assert_eq!(
            extract_title("<p>标题</p><p>正文第一行</p><p>正文第二行</p>"),
            "标题"
        );
        assert_eq!(extract_title("<p></p>"), "Untitled");
        assert_eq!(extract_title(&"x".repeat(120)).chars().count(), 100);
    }

    #[test]
    fn extracts_unique_attachment_ids() {
        let ids = collect_attachment_candidates([
            r#"<p><img src="attachment://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"></p>"#,
            r#"<p><img src="attachment://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"></p>"#,
            r#"<p><img src="attachment://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"></p>"#,
        ]);
        assert_eq!(
            ids,
            vec![
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            ]
        );
    }
}
