use super::*;

pub enum ChangePayload {
    None,
    Note(Note),
    Attachment(AttachmentRecord),
    Clipboard(ClipboardItem),
    Tag(Tag),
    NoteTag(NoteTag),
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
    let (causal_version, _) =
        next_causal_version(tx, user_id, entity_type, entity_id, None).await?;
    let (note, attachment, clipboard, tag, note_tag) = match payload {
        ChangePayload::None => (None, None, None, None, None),
        ChangePayload::Note(value) => (Some(value), None, None, None, None),
        ChangePayload::Attachment(value) => (None, Some(value), None, None, None),
        ChangePayload::Clipboard(value) => (None, None, Some(value), None, None),
        ChangePayload::Tag(value) => (None, None, None, Some(value), None),
        ChangePayload::NoteTag(value) => (None, None, None, None, Some(value)),
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
        yjs_update: None,
        note,
        attachment,
        clipboard,
        tag,
        note_tag,
    };
    let envelope_json = serde_json::to_value(&envelope)
        .map_err(|error| AppError::Internal(format!("Serialize error: {error}")))?;
    sqlx::query(
        "INSERT INTO cloud_changes
            (user_id, seq, entity_type, entity_id, operation, source_device, source_seq, envelope, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, 'cloud', $2, $6, $1, $1)",
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

pub(super) async fn next_causal_version(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    entity_type: &str,
    entity_id: &str,
    incoming: Option<&CausalVersion>,
) -> Result<(CausalVersion, CausalRelation), AppError> {
    let empty = serde_json::to_value(CausalVersion::default())
        .map_err(|error| AppError::Internal(error.to_string()))?;
    sqlx::query(
        "INSERT INTO entity_versions
            (user_id,entity_type,entity_id,version,created_by,updated_by)
         VALUES ($1,$2,$3,$4,$1,$1)
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
    let relation = incoming
        .map(|value| stored.relation(value))
        .unwrap_or(CausalRelation::Equal);
    let mut next = match incoming {
        Some(incoming) => stored.merge_with_winner(incoming, incoming),
        None => stored,
    };
    next.increment("cloud");
    let encoded =
        serde_json::to_value(&next).map_err(|error| AppError::Internal(error.to_string()))?;
    sqlx::query(
        "UPDATE entity_versions
         SET version=$4, updated_at=NOW(), updated_by=$1
         WHERE user_id=$1 AND entity_type=$2 AND entity_id=$3",
    )
    .bind(user_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(encoded)
    .execute(&mut **tx)
    .await?;
    Ok((next, relation))
}

pub(super) async fn preserve_note_conflict(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    note_id: &str,
    source_device: &str,
    source_seq: i64,
) -> Result<(), AppError> {
    use sha2::{Digest, Sha256};
    let conflict_id = format!(
        "{:x}",
        Sha256::digest(format!(
            "cloud-conflict:{note_id}:{source_device}:{source_seq}"
        )),
    );
    let source = &source_device[..source_device.len().min(8)];
    let inserted = sqlx::query(
        "INSERT INTO notes
            (user_id,id,title,content,yjs_state,yjs_state_version,is_pinned,sort_order,
             created_at,updated_at,version,is_deleted,created_by,updated_by)
         SELECT user_id,$3,title || ' (conflict ' || $4 || ')',content,yjs_state,
                yjs_state_version,false,sort_order,created_at,updated_at,version,false,$1,$1
         FROM notes WHERE user_id=$1 AND id=$2
         ON CONFLICT(user_id,id) DO NOTHING",
    )
    .bind(user_id)
    .bind(note_id)
    .bind(&conflict_id)
    .bind(source)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if inserted > 0 {
        let conflict: Note = sqlx::query_as(
            "SELECT id,title,content,yjs_state,yjs_state_version,is_pinned,sort_order,
                    created_at,updated_at,version,is_deleted,ARRAY[]::TEXT[] AS tags
             FROM notes WHERE user_id=$1 AND id=$2",
        )
        .bind(user_id)
        .bind(&conflict_id)
        .fetch_one(&mut **tx)
        .await?;
        append_change(
            tx,
            user_id,
            "note",
            &conflict_id,
            "upsert",
            ChangePayload::Note(conflict),
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn merge_concurrent_yjs_note(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    envelope: &mut SyncEnvelope,
) -> Result<bool, AppError> {
    use yrs::updates::decoder::Decode;
    use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

    let Some(note) = envelope.note.as_mut() else {
        return Ok(false);
    };
    let Some(remote_state) = note.yjs_state.as_ref() else {
        return Ok(false);
    };
    let local_state: Option<Option<Vec<u8>>> =
        sqlx::query_scalar("SELECT yjs_state FROM notes WHERE user_id=$1 AND id=$2")
            .bind(user_id)
            .bind(&note.id)
            .fetch_optional(&mut **tx)
            .await?;
    let local_state = local_state.flatten();
    let Some(local_state) = local_state else {
        return Ok(false);
    };
    let doc = Doc::new();
    for bytes in [&local_state, remote_state] {
        let update = Update::decode_v1(bytes)
            .map_err(|error| AppError::BadRequest(format!("Invalid Yjs state: {error}")))?;
        doc.transact_mut()
            .apply_update(update)
            .map_err(|error| AppError::BadRequest(format!("Cannot merge Yjs state: {error}")))?;
    }
    note.yjs_state = Some(
        doc.transact()
            .encode_state_as_update_v1(&StateVector::default()),
    );
    note.yjs_state_version += 1;
    Ok(true)
}
