use crate::error::AppError;
use crate::models::Note;
use crate::routes::sync::{append_change, ChangePayload};
use crate::AppState;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use uuid::Uuid;
use yrs::updates::decoder::Decode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

pub type StoredYjsState = (Option<Vec<u8>>, i64, Option<chrono::DateTime<chrono::Utc>>);

pub enum ProjectionResult {
    Accepted(i64),
    Stale(i64),
}

pub async fn load_note_state(
    state: &AppState,
    user_id: Uuid,
    note_id: &str,
) -> Result<Option<(Vec<u8>, i64, bool)>, AppError> {
    let mut tx = state.db.inner().begin().await?;
    let stored: Option<StoredYjsState> = sqlx::query_as(
        "SELECT yjs_state,yjs_state_version,yjs_bootstrap_at
         FROM notes WHERE user_id=$1 AND id=$2 AND is_deleted=false FOR UPDATE",
    )
    .bind(user_id)
    .bind(note_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((stored, version, bootstrap_at)) = stored else {
        tx.rollback().await?;
        return Ok(None);
    };
    let lease_expired = bootstrap_at
        .map(|claimed| chrono::Utc::now() - claimed > chrono::Duration::seconds(30))
        .unwrap_or(true);
    let bootstrap = version == 0 && lease_expired;
    let bytes = stored.unwrap_or_else(empty_yjs_state);
    if bootstrap {
        sqlx::query(
            "UPDATE notes SET yjs_state=$3,yjs_bootstrap_at=NOW() WHERE user_id=$1 AND id=$2",
        )
        .bind(user_id)
        .bind(note_id)
        .bind(&bytes)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Some((bytes, version, bootstrap)))
}

pub async fn persist_update(
    state: &AppState,
    user_id: Uuid,
    note_id: &str,
    client_id: &str,
    update_id: Uuid,
    update: &[u8],
) -> Result<i64, AppError> {
    let mut tx = state.db.inner().begin().await?;
    let existing: Option<(Option<Vec<u8>>, i64)> = sqlx::query_as(
        "SELECT yjs_state,yjs_state_version FROM notes
         WHERE user_id=$1 AND id=$2 AND is_deleted=false FOR UPDATE",
    )
    .bind(user_id)
    .bind(note_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((existing, version)) = existing else {
        tx.rollback().await?;
        return Err(AppError::NotFound);
    };
    let delivered: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM yjs_updates
         WHERE user_id=$1 AND note_id=$2 AND update_id=$3)",
    )
    .bind(user_id)
    .bind(note_id)
    .bind(update_id)
    .fetch_one(&mut *tx)
    .await?;
    if delivered {
        tx.commit().await?;
        return Ok(version);
    }
    let compacted = merge_update(existing.as_deref(), update)?;
    let changed = existing.as_deref() != Some(compacted.as_slice());
    sqlx::query(
        "INSERT INTO yjs_updates(user_id,note_id,update,source_client_id,update_id)
         VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(user_id)
    .bind(note_id)
    .bind(update)
    .bind(client_id)
    .bind(update_id)
    .execute(&mut *tx)
    .await?;
    let next_version = version + i64::from(changed);
    if changed {
        sqlx::query(
            "UPDATE notes SET yjs_state=$3,yjs_state_version=$4,updated_at=$5,updated_by=$1
             WHERE user_id=$1 AND id=$2",
        )
        .bind(user_id)
        .bind(note_id)
        .bind(compacted)
        .bind(next_version)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "DELETE FROM yjs_updates WHERE user_id=$1 AND note_id=$2 AND id < (
         SELECT COALESCE(MAX(id),0)-500 FROM yjs_updates WHERE user_id=$1 AND note_id=$2)",
    )
    .bind(user_id)
    .bind(note_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(next_version)
}

pub async fn persist_projection(
    state: &AppState,
    user_id: Uuid,
    note_id: &str,
    html: String,
    encoded_vector: &str,
) -> Result<ProjectionResult, AppError> {
    let requested_vector = STANDARD
        .decode(encoded_vector)
        .map_err(|_| AppError::BadRequest("Invalid Yjs state vector".into()))?;
    let requested_vector = StateVector::decode_v1(&requested_vector)
        .map_err(|_| AppError::BadRequest("Invalid Yjs state vector".into()))?;
    let mut tx = state.db.inner().begin().await?;
    let stored: Option<(Vec<u8>, i64)> = sqlx::query_as(
        "SELECT yjs_state,yjs_state_version FROM notes
         WHERE user_id=$1 AND id=$2 AND is_deleted=false AND yjs_state IS NOT NULL FOR UPDATE",
    )
    .bind(user_id)
    .bind(note_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((stored, state_version)) = stored else {
        tx.rollback().await?;
        return Err(AppError::NotFound);
    };
    if state_vector(&stored)? != requested_vector {
        tx.rollback().await?;
        return Ok(ProjectionResult::Stale(state_version));
    }
    save_projection(&mut tx, user_id, note_id, html).await?;
    tx.commit().await?;
    let _ = state.event_tx.send(crate::models::SyncEvent {
        user_id,
        entity_type: "note".into(),
        entity_id: note_id.into(),
        operation: "upsert".into(),
    });
    Ok(ProjectionResult::Accepted(state_version))
}

async fn save_projection(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    note_id: &str,
    html: String,
) -> Result<(), AppError> {
    let (title, search_text) = projection_text(&html);
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO note_versions
         (note_id,user_id,title,content,version,created_at,updated_at,is_pinned,created_by,updated_by)
         SELECT id,user_id,title,content,version,updated_at,updated_at,false,$1,$1 FROM notes n
         WHERE user_id=$1 AND id=$2 AND NOT EXISTS (SELECT 1 FROM note_versions v
         WHERE v.user_id=$1 AND v.note_id=$2 AND v.created_at::timestamptz>NOW()-INTERVAL '30 seconds')",
    )
    .bind(user_id)
    .bind(note_id)
    .execute(&mut **tx)
    .await?;
    let note: Note = sqlx::query_as(
        "UPDATE notes SET content=$3,title=$4,search_text=$5,updated_at=$6,version=version+1,updated_by=$1
         WHERE user_id=$1 AND id=$2 RETURNING id,title,content,yjs_state,yjs_state_version,is_pinned,
         sort_order,created_at,updated_at,version,is_deleted,ARRAY[]::TEXT[] AS tags",
    )
    .bind(user_id)
    .bind(note_id)
    .bind(html)
    .bind(title)
    .bind(search_text)
    .bind(now)
    .fetch_one(&mut **tx)
    .await?;
    append_change(
        tx,
        user_id,
        "note",
        note_id,
        "upsert",
        ChangePayload::Note(note),
    )
    .await?;
    Ok(())
}

fn empty_yjs_state() -> Vec<u8> {
    Doc::new()
        .transact()
        .encode_state_as_update_v1(&StateVector::default())
}

fn state_vector(update: &[u8]) -> Result<StateVector, AppError> {
    let doc = Doc::new();
    doc.transact_mut()
        .apply_update(Update::decode_v1(update).map_err(invalid_update)?)
        .map_err(|error| AppError::Internal(format!("Apply stored Yjs state failed: {error}")))?;
    let vector = doc.transact().state_vector();
    Ok(vector)
}

fn merge_update(existing: Option<&[u8]>, incoming: &[u8]) -> Result<Vec<u8>, AppError> {
    let doc = Doc::new();
    if let Some(existing) = existing {
        doc.transact_mut()
            .apply_update(Update::decode_v1(existing).map_err(invalid_update)?)
            .map_err(|error| {
                AppError::Internal(format!("Apply stored Yjs state failed: {error}"))
            })?;
    }
    doc.transact_mut()
        .apply_update(Update::decode_v1(incoming).map_err(invalid_update)?)
        .map_err(|error| AppError::BadRequest(format!("Apply Yjs update failed: {error}")))?;
    let compacted = doc
        .transact()
        .encode_state_as_update_v1(&StateVector::default());
    Ok(compacted)
}

fn invalid_update(error: impl std::fmt::Display) -> AppError {
    AppError::BadRequest(format!("Invalid Yjs update: {error}"))
}

fn projection_text(html: &str) -> (String, String) {
    let text = regex::Regex::new(r"<[^>]*>")
        .expect("valid regex")
        .replace_all(html, " ");
    let search_text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let title: String = search_text.chars().take(100).collect();
    (
        if title.is_empty() {
            "Untitled".into()
        } else {
            title
        },
        search_text,
    )
}
