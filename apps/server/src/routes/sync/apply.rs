use super::*;

pub(super) async fn apply_to_canonical(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    envelope: &SyncEnvelope,
) -> Result<(), AppError> {
    match (envelope.entity_type.as_str(), envelope.operation.as_str()) {
        ("note", "delete") => {
            sqlx::query("UPDATE notes SET is_deleted = true, updated_at = $3, updated_by = $1 WHERE user_id = $1 AND id = $2")
                .bind(user_id).bind(&envelope.entity_id).bind(&envelope.changed_at)
                .execute(&mut **tx).await?;
        }
        ("note", "upsert") => {
            let note = envelope
                .note
                .as_ref()
                .ok_or_else(|| AppError::BadRequest("Note upsert is missing its payload".into()))?;
            sqlx::query(
                "INSERT INTO notes
                    (user_id, id, title, content, search_text, yjs_state, yjs_state_version, is_pinned, sort_order, created_at, updated_at, version, is_deleted, created_by, updated_by)
                 VALUES ($1,$2,$3,$4,trim(regexp_replace($4, '<[^>]+>', ' ', 'g')),$5,$6,$7,$8,$9,$10,$11,$12,$1,$1)
                  ON CONFLICT (user_id,id) DO UPDATE SET title=EXCLUDED.title, content=EXCLUDED.content,
                 search_text=EXCLUDED.search_text,
                 yjs_state=EXCLUDED.yjs_state, yjs_state_version=EXCLUDED.yjs_state_version,
                 is_pinned=EXCLUDED.is_pinned, sort_order=EXCLUDED.sort_order, updated_at=EXCLUDED.updated_at,
                 version=EXCLUDED.version, is_deleted=EXCLUDED.is_deleted, updated_by=EXCLUDED.updated_by",
            ).bind(user_id).bind(&note.id).bind(&note.title).bind(&note.content).bind(&note.yjs_state)
                .bind(note.yjs_state_version).bind(note.is_pinned)
                .bind(note.sort_order).bind(&note.created_at).bind(&note.updated_at).bind(note.version).bind(note.is_deleted)
                .execute(&mut **tx).await?;
        }
        ("clipboard", "delete") => {
            sqlx::query("UPDATE clipboard_items SET is_deleted = true, updated_at = $3, updated_by = $1 WHERE user_id = $1 AND id = $2")
                .bind(user_id).bind(&envelope.entity_id).bind(&envelope.changed_at)
                .execute(&mut **tx).await?;
        }
        ("clipboard", "upsert") => {
            let item = envelope.clipboard.as_ref().ok_or_else(|| {
                AppError::BadRequest("Clipboard upsert is missing its payload".into())
            })?;
            sqlx::query(
                "INSERT INTO clipboard_items
                    (user_id,id,kind,content,preview,source_device,created_at,updated_at,last_copied_at,capture_count,is_pinned,is_deleted,created_by,updated_by)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$1,$1)
                  ON CONFLICT (user_id,id) DO UPDATE SET kind=EXCLUDED.kind, content=EXCLUDED.content,
                  preview=EXCLUDED.preview, source_device=EXCLUDED.source_device, updated_at=EXCLUDED.updated_at,
                  last_copied_at=EXCLUDED.last_copied_at, capture_count=EXCLUDED.capture_count,
                  is_pinned=EXCLUDED.is_pinned, is_deleted=EXCLUDED.is_deleted, updated_by=EXCLUDED.updated_by",
            ).bind(user_id).bind(&item.id).bind(&item.kind).bind(&item.content).bind(&item.preview)
                .bind(&item.source_device).bind(&item.created_at).bind(&item.updated_at)
                .bind(&item.last_copied_at).bind(item.capture_count).bind(item.is_pinned).bind(item.is_deleted)
                .execute(&mut **tx).await?;
            sqlx::query(
                "DELETE FROM clipboard_items WHERE user_id=$1 AND id IN (
                    SELECT id FROM clipboard_items WHERE user_id=$1 AND is_deleted=false
                    ORDER BY is_pinned DESC,last_copied_at DESC,created_at DESC,id DESC
                    OFFSET 500
                 )",
            )
            .bind(user_id)
            .execute(&mut **tx)
            .await?;
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
        ("tag", "delete") => {
            sqlx::query("UPDATE tags SET is_deleted=true, updated_at=$3, updated_by=$1 WHERE user_id=$1 AND id=$2")
                .bind(user_id)
                .bind(&envelope.entity_id)
                .bind(&envelope.changed_at)
                .execute(&mut **tx)
                .await?;
        }
        ("tag", "upsert") => {
            let tag = envelope
                .tag
                .as_ref()
                .ok_or_else(|| AppError::BadRequest("Tag upsert is missing its payload".into()))?;
            sqlx::query(
                "INSERT INTO tags
                    (user_id,id,name,normalized_name,color,created_at,updated_at,is_deleted,created_by,updated_by)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$1,$1)
                 ON CONFLICT(user_id,id) DO UPDATE SET
                    name=EXCLUDED.name, normalized_name=EXCLUDED.normalized_name,
                    color=EXCLUDED.color, updated_at=EXCLUDED.updated_at,
                    is_deleted=EXCLUDED.is_deleted, updated_by=EXCLUDED.updated_by",
            )
            .bind(user_id)
            .bind(&tag.id)
            .bind(&tag.name)
            .bind(&tag.normalized_name)
            .bind(&tag.color)
            .bind(&tag.created_at)
            .bind(&tag.updated_at)
            .bind(tag.is_deleted)
            .execute(&mut **tx)
            .await?;
        }
        ("note_tag", "delete") => {
            sqlx::query("DELETE FROM note_tags WHERE user_id=$1 AND id=$2")
                .bind(user_id)
                .bind(&envelope.entity_id)
                .execute(&mut **tx)
                .await?;
        }
        ("note_tag", "upsert") => {
            let note_tag = envelope.note_tag.as_ref().ok_or_else(|| {
                AppError::BadRequest("Note-tag upsert is missing its payload".into())
            })?;
            sqlx::query(
                "INSERT INTO note_tags(id,user_id,note_id,tag_id,created_at,created_by,updated_by)
                 VALUES ($1,$2,$3,$4,$5,$2,$2)
                 ON CONFLICT(user_id,id) DO NOTHING",
            )
            .bind(&note_tag.id)
            .bind(user_id)
            .bind(&note_tag.note_id)
            .bind(&note_tag.tag_id)
            .bind(&note_tag.created_at)
            .execute(&mut **tx)
            .await?;
        }
        _ => return Err(AppError::BadRequest("Unsupported sync operation".into())),
    }
    Ok(())
}

pub(super) async fn touch_sync_cursor(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    device_id: &str,
    cursor_seq: i64,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO sync_cursors
            (user_id, device_id, cursor_seq, created_at, updated_at, created_by, updated_by)
         VALUES ($1, $2, $3, NOW(), NOW(), $1, $1)
         ON CONFLICT (user_id, device_id)
         DO UPDATE SET
            cursor_seq = GREATEST(sync_cursors.cursor_seq, EXCLUDED.cursor_seq),
            updated_at = NOW(),
            updated_by = EXCLUDED.updated_by",
    )
    .bind(user_id)
    .bind(device_id)
    .bind(cursor_seq)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
