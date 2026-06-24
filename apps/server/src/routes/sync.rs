use crate::error::AppError;
use crate::middleware::AuthUser;
use crate::models::{
    AttachmentRecord, CausalVersion, ClipboardItem, CloudChange, Note, PullRequest, PullResponse,
    PushRequest, PushResponse, SyncEnvelope, SyncEvent,
};
use crate::AppState;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::Json;
use futures::stream::Stream;
use sqlx::{Postgres, Transaction};
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::StreamExt as _;
use uuid::Uuid;

pub async fn pull(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<PullRequest>,
) -> Result<Json<PullResponse>, AppError> {
    if req.since_seq < 0 {
        return Err(AppError::BadRequest(
            "since_seq must be non-negative".into(),
        ));
    }

    let changes: Vec<CloudChange> = sqlx::query_as(
        "SELECT * FROM cloud_changes WHERE user_id = $1 AND seq > $2 ORDER BY seq ASC LIMIT 500",
    )
    .bind(user_id)
    .bind(req.since_seq)
    .fetch_all(state.db.inner())
    .await?;

    let server_seq = changes
        .last()
        .map(|change| change.seq)
        .unwrap_or(req.since_seq);
    let envelopes = changes
        .into_iter()
        .map(|change| {
            serde_json::from_value(change.envelope).map_err(|error| {
                AppError::Internal(format!(
                    "Invalid stored sync envelope at seq {}: {error}",
                    change.seq
                ))
            })
        })
        .collect::<Result<Vec<SyncEnvelope>, AppError>>()?;

    Ok(Json(PullResponse {
        envelopes,
        server_seq,
    }))
}

pub async fn push(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<PushRequest>,
) -> Result<Json<PushResponse>, AppError> {
    if req.envelopes.len() > 500 {
        return Err(AppError::BadRequest(
            "A push is limited to 500 changes".into(),
        ));
    }

    let mut accepted = 0;
    let mut conflicts = 0;
    let mut acknowledged_sequences = Vec::with_capacity(req.envelopes.len());

    for envelope in &req.envelopes {
        validate_envelope(envelope)?;
        let mut tx = state.db.inner().begin().await?;
        let causal_version = next_causal_version(
            &mut tx,
            user_id,
            &envelope.entity_type,
            &envelope.entity_id,
            envelope.causal_version.as_ref(),
        )
        .await?;
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM cloud_changes WHERE user_id=$1 AND source_device=$2 AND source_seq=$3)",
        ).bind(user_id).bind(&envelope.device_id).bind(envelope.seq).fetch_one(&mut *tx).await?;
        if duplicate {
            acknowledged_sequences.push(envelope.seq);
            conflicts += 1;
            tx.rollback().await?;
            continue;
        }
        let (server_seq,): (i64,) = sqlx::query_as("SELECT nextval('cloud_changes_seq')")
            .fetch_one(&mut *tx)
            .await?;
        let mut canonical = envelope.clone();
        canonical.device_id = "cloud".to_string();
        canonical.seq = server_seq;
        canonical.causal_version = Some(causal_version);
        let envelope_json = serde_json::to_value(&canonical)
            .map_err(|error| AppError::Internal(format!("Serialize error: {error}")))?;

        let result = sqlx::query(
            "INSERT INTO cloud_changes
                (user_id, seq, entity_type, entity_id, operation, source_device, source_seq, envelope)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (user_id, source_device, source_seq) DO NOTHING",
        )
        .bind(user_id)
        .bind(server_seq)
        .bind(&envelope.entity_type)
        .bind(&envelope.entity_id)
        .bind(&envelope.operation)
        .bind(&envelope.device_id)
        .bind(envelope.seq)
        .bind(&envelope_json)
        .execute(&mut *tx)
        .await?;

        debug_assert_eq!(result.rows_affected(), 1);

        apply_to_canonical(&mut tx, user_id, envelope).await?;
        tx.commit().await?;
        acknowledged_sequences.push(envelope.seq);
        accepted += 1;
    }

    if accepted > 0 {
        let _ = state.event_tx.send(SyncEvent {
            user_id,
            entity_type: "batch".to_string(),
            entity_id: String::new(),
            operation: "push".to_string(),
        });
    }

    Ok(Json(PushResponse {
        accepted,
        conflicts,
        acknowledged_sequences,
    }))
}

pub enum ChangePayload {
    None,
    Note(Note),
    Attachment(AttachmentRecord),
    Clipboard(ClipboardItem),
}

pub async fn append_change(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    entity_type: &str,
    entity_id: &str,
    operation: &str,
    payload: ChangePayload,
) -> Result<(), AppError> {
    let (seq,): (i64,) = sqlx::query_as("SELECT nextval('cloud_changes_seq')")
        .fetch_one(&mut **tx)
        .await?;
    let causal_version = next_causal_version(tx, user_id, entity_type, entity_id, None).await?;
    let (note, attachment, clipboard) = match payload {
        ChangePayload::None => (None, None, None),
        ChangePayload::Note(value) => (Some(value), None, None),
        ChangePayload::Attachment(value) => (None, Some(value), None),
        ChangePayload::Clipboard(value) => (None, None, Some(value)),
    };
    let envelope = SyncEnvelope {
        schema_version: 2,
        device_id: "cloud".to_string(),
        seq,
        entity_type: entity_type.to_string(),
        entity_id: entity_id.to_string(),
        operation: operation.to_string(),
        changed_at: chrono::Utc::now().to_rfc3339(),
        causal_version: Some(causal_version),
        note,
        attachment,
        clipboard,
    };
    let envelope_json = serde_json::to_value(&envelope)
        .map_err(|error| AppError::Internal(format!("Serialize error: {error}")))?;
    sqlx::query(
        "INSERT INTO cloud_changes
            (user_id, seq, entity_type, entity_id, operation, source_device, source_seq, envelope)
         VALUES ($1, $2, $3, $4, $5, 'cloud', $2, $6)",
    )
    .bind(user_id)
    .bind(seq)
    .bind(entity_type)
    .bind(entity_id)
    .bind(operation)
    .bind(envelope_json)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn next_causal_version(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    entity_type: &str,
    entity_id: &str,
    incoming: Option<&CausalVersion>,
) -> Result<CausalVersion, AppError> {
    let empty = serde_json::to_value(CausalVersion::default())
        .map_err(|error| AppError::Internal(error.to_string()))?;
    sqlx::query(
        "INSERT INTO entity_versions (user_id,entity_type,entity_id,version) VALUES ($1,$2,$3,$4)
         ON CONFLICT (user_id,entity_type,entity_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(empty)
    .execute(&mut **tx)
    .await?;
    let stored: serde_json::Value = sqlx::query_scalar(
        "SELECT version FROM entity_versions WHERE user_id=$1 AND entity_type=$2 AND entity_id=$3 FOR UPDATE",
    ).bind(user_id).bind(entity_type).bind(entity_id).fetch_one(&mut **tx).await?;
    let stored: CausalVersion = serde_json::from_value(stored)
        .map_err(|error| AppError::Internal(format!("Invalid stored causal version: {error}")))?;
    let mut next = match incoming {
        Some(incoming) => stored.merge_with_winner(incoming, incoming),
        None => stored,
    };
    next.increment("cloud");
    let encoded =
        serde_json::to_value(&next).map_err(|error| AppError::Internal(error.to_string()))?;
    sqlx::query("UPDATE entity_versions SET version=$4 WHERE user_id=$1 AND entity_type=$2 AND entity_id=$3")
        .bind(user_id).bind(entity_type).bind(entity_id).bind(encoded).execute(&mut **tx).await?;
    Ok(next)
}

async fn apply_to_canonical(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    envelope: &SyncEnvelope,
) -> Result<(), AppError> {
    match (envelope.entity_type.as_str(), envelope.operation.as_str()) {
        ("note", "delete") => {
            sqlx::query("UPDATE notes SET is_deleted = true, updated_at = $3 WHERE user_id = $1 AND id = $2")
                .bind(user_id).bind(&envelope.entity_id).bind(&envelope.changed_at)
                .execute(&mut **tx).await?;
        }
        ("note", "upsert") => {
            let note = envelope
                .note
                .as_ref()
                .ok_or_else(|| AppError::BadRequest("Note upsert is missing its payload".into()))?;
            sqlx::query(
                "INSERT INTO notes (user_id, id, title, content, is_pinned, sort_order, created_at, updated_at, version, is_deleted)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                 ON CONFLICT (user_id,id) DO UPDATE SET title=EXCLUDED.title, content=EXCLUDED.content,
                 is_pinned=EXCLUDED.is_pinned, sort_order=EXCLUDED.sort_order, updated_at=EXCLUDED.updated_at,
                 version=EXCLUDED.version, is_deleted=EXCLUDED.is_deleted",
            ).bind(user_id).bind(&note.id).bind(&note.title).bind(&note.content).bind(note.is_pinned)
                .bind(note.sort_order).bind(&note.created_at).bind(&note.updated_at).bind(note.version).bind(note.is_deleted)
                .execute(&mut **tx).await?;
        }
        ("clipboard", "delete") => {
            sqlx::query("UPDATE clipboard_items SET is_deleted = true, updated_at = $3 WHERE user_id = $1 AND id = $2")
                .bind(user_id).bind(&envelope.entity_id).bind(&envelope.changed_at)
                .execute(&mut **tx).await?;
        }
        ("clipboard", "upsert") => {
            let item = envelope.clipboard.as_ref().ok_or_else(|| {
                AppError::BadRequest("Clipboard upsert is missing its payload".into())
            })?;
            sqlx::query(
                "INSERT INTO clipboard_items (user_id,id,kind,content,preview,source_device,created_at,updated_at,last_copied_at,capture_count,is_pinned,is_deleted)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                 ON CONFLICT (user_id,id) DO UPDATE SET kind=EXCLUDED.kind, content=EXCLUDED.content,
                 preview=EXCLUDED.preview, source_device=EXCLUDED.source_device, updated_at=EXCLUDED.updated_at,
                 last_copied_at=EXCLUDED.last_copied_at, capture_count=EXCLUDED.capture_count,
                 is_pinned=EXCLUDED.is_pinned, is_deleted=EXCLUDED.is_deleted",
            ).bind(user_id).bind(&item.id).bind(&item.kind).bind(&item.content).bind(&item.preview)
                .bind(&item.source_device).bind(&item.created_at).bind(&item.updated_at)
                .bind(&item.last_copied_at).bind(item.capture_count).bind(item.is_pinned).bind(item.is_deleted)
                .execute(&mut **tx).await?;
        }
        ("attachment", "delete") => {
            sqlx::query("DELETE FROM attachments WHERE user_id = $1 AND id = $2")
                .bind(user_id)
                .bind(&envelope.entity_id)
                .execute(&mut **tx)
                .await?;
        }
        ("attachment", "upsert") => {
            let item = envelope.attachment.as_ref().ok_or_else(|| {
                AppError::BadRequest("Attachment upsert is missing its payload".into())
            })?;
            let stored: Option<(i64,)> =
                sqlx::query_as("SELECT size FROM attachments WHERE user_id=$1 AND id=$2")
                    .bind(user_id)
                    .bind(&item.id)
                    .fetch_optional(&mut **tx)
                    .await?;
            if stored != Some((item.size,)) {
                return Err(AppError::BadRequest(
                    "Attachment content must be uploaded before its sync envelope".into(),
                ));
            }
        }
        _ => return Err(AppError::BadRequest("Unsupported sync operation".into())),
    }
    Ok(())
}

fn validate_envelope(envelope: &SyncEnvelope) -> Result<(), AppError> {
    if envelope.seq <= 0 || envelope.device_id.is_empty() || envelope.entity_id.is_empty() {
        return Err(AppError::BadRequest("Invalid envelope identity".into()));
    }
    if envelope.schema_version != 2 || envelope.causal_version.is_none() {
        return Err(AppError::BadRequest(
            "Cloud sync requires schema version 2".into(),
        ));
    }
    if !matches!(
        envelope.entity_type.as_str(),
        "note" | "attachment" | "clipboard"
    ) || !matches!(envelope.operation.as_str(), "upsert" | "delete")
    {
        return Err(AppError::BadRequest(
            "Unsupported envelope type or operation".into(),
        ));
    }
    let payload_matches = match (envelope.entity_type.as_str(), envelope.operation.as_str()) {
        ("note", "upsert") => {
            envelope.note.as_ref().map(|item| item.id.as_str()) == Some(envelope.entity_id.as_str())
        }
        ("attachment", "upsert") => {
            envelope.attachment.as_ref().map(|item| item.id.as_str())
                == Some(envelope.entity_id.as_str())
        }
        ("clipboard", _) => {
            envelope.clipboard.as_ref().map(|item| item.id.as_str())
                == Some(envelope.entity_id.as_str())
        }
        _ => true,
    };
    if !payload_matches {
        return Err(AppError::BadRequest(
            "Envelope payload does not match its entity ID".into(),
        ));
    }
    Ok(())
}

pub async fn events_sse(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_tx.subscribe();
    let stream =
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(move |result| match result {
            Ok(event) if event.user_id == user_id => {
                Some(Ok(Event::default().json_data(event).unwrap_or_default()))
            }
            _ => None,
        });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn envelope() -> SyncEnvelope {
        SyncEnvelope {
            schema_version: 2,
            device_id: "desktop-a".into(),
            seq: 7,
            entity_type: "note".into(),
            entity_id: "note-a".into(),
            operation: "upsert".into(),
            changed_at: "2026-01-01T00:00:00Z".into(),
            causal_version: Some(CausalVersion {
                counters: BTreeMap::from([("desktop-a".into(), 1)]),
                origin: "desktop-a".into(),
            }),
            note: Some(Note {
                id: "note-a".into(),
                title: "A".into(),
                content: "<p>A</p>".into(),
                is_pinned: false,
                sort_order: 0,
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                version: 1,
                is_deleted: false,
            }),
            attachment: None,
            clipboard: None,
        }
    }

    #[test]
    fn accepts_canonical_v2_envelope() {
        assert!(validate_envelope(&envelope()).is_ok());
    }

    #[test]
    fn rejects_missing_causal_version() {
        let mut value = envelope();
        value.causal_version = None;
        assert!(validate_envelope(&value).is_err());
    }

    #[test]
    fn rejects_unknown_entity_type() {
        let mut value = envelope();
        value.entity_type = "unknown".into();
        assert!(validate_envelope(&value).is_err());
    }
}
