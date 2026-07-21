use super::*;

#[allow(dead_code)]
pub(super) fn state_file_path(device_id: &str, entity_type: &str, entity_id: &str) -> String {
    format!("state/{device_id}/{entity_type}/{entity_id}.json")
}

#[cfg(test)]
pub(super) fn revision_file_path(device_id: &str) -> String {
    format!("state/{device_id}/meta/revision")
}

#[cfg(test)]
pub(super) async fn push_state(
    provider: &dyn SyncProvider,
    db: &Database,
    attachments_dir: &Path,
    config: &SyncConfig,
) -> Result<usize, String> {
    let mut pushed = 0;
    loop {
        let changes = db
            .list_pending_changes(WEBDAV_BATCH_ENTITY_LIMIT)
            .map_err(|e| e.to_string())?;
        if changes.is_empty() {
            break;
        }

        let mut seen = std::collections::HashSet::new();
        let mut entities: Vec<(String, String)> = Vec::new();
        for change in &changes {
            let key = (change.entity_type.clone(), change.entity_id.clone());
            if seen.insert(key.clone()) {
                entities.push(key);
            }
        }
        // Publish lightweight note/metadata state before bandwidth-heavy attachments.
        if entities
            .iter()
            .any(|(entity_type, _)| entity_type != "attachment")
        {
            entities.retain(|(entity_type, _)| entity_type != "attachment");
        }

        let mut envelopes = Vec::with_capacity(entities.len());
        for (entity_type, entity_id) in &entities {
            envelopes.push(build_state_envelope(
                db,
                entity_type,
                entity_id,
                &config.device_id,
            )?);
        }

        // Keep batches bounded. A single large note is still sent by itself.
        while envelopes.len() > 1
            && serde_json::to_vec(&envelopes)
                .map_err(|error| error.to_string())?
                .len()
                > WEBDAV_BATCH_BYTES_LIMIT
        {
            envelopes.pop();
            entities.pop();
        }

        for envelope in &envelopes {
            if let Some(attachment) = &envelope.attachment {
                upload_attachment(provider, attachments_dir, attachment).await?;
            }
        }

        let envelope_bytes = serde_json::to_vec(&envelopes).map_err(|error| error.to_string())?;
        let batch_hash = format!("{:x}", Sha256::digest(&envelope_bytes));
        let head_path = device_head_path(&config.device_id);
        let current_head = read_device_head(provider, &config.device_id).await?;

        // A lost response after committing the head must not create a duplicate generation.
        if current_head
            .as_ref()
            .is_some_and(|head| head.batch_hash == batch_hash)
        {
            mark_batch_synced(db, &changes, &entities)?;
            pushed += entities.len();
            continue;
        }

        let generation = current_head.map_or(1, |head| head.generation + 1);
        let batch = WebDavBatch {
            schema_version: 1,
            device_id: config.device_id.clone(),
            generation,
            envelopes,
        };
        provider
            .put(
                &batch_path(&config.device_id, generation),
                gzip_json(&batch)?,
                "application/gzip",
            )
            .await?;
        let head = WebDavHead {
            schema_version: 1,
            device_id: config.device_id.clone(),
            generation,
            batch_hash,
        };
        provider
            .put(
                &head_path,
                serde_json::to_vec(&head).map_err(|error| error.to_string())?,
                "application/json",
            )
            .await?;

        mark_batch_synced(db, &changes, &entities)?;
        pushed += entities.len();
    }

    let head_path = device_head_path(&config.device_id);
    if pushed == 0 && provider.get(&head_path).await?.is_none() {
        let head = WebDavHead {
            schema_version: 1,
            device_id: config.device_id.clone(),
            generation: 0,
            batch_hash: String::new(),
        };
        provider
            .put(
                &head_path,
                serde_json::to_vec(&head).map_err(|error| error.to_string())?,
                "application/json",
            )
            .await?;
    }
    Ok(pushed)
}

#[cfg(test)]
fn mark_batch_synced(
    db: &Database,
    changes: &[SyncChange],
    entities: &[(String, String)],
) -> Result<(), String> {
    let included: std::collections::HashSet<_> = entities.iter().cloned().collect();
    for change in changes {
        if included.contains(&(change.entity_type.clone(), change.entity_id.clone())) {
            db.mark_change_synced(change.seq)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) async fn read_device_head(
    provider: &dyn SyncProvider,
    device_id: &str,
) -> Result<Option<WebDavHead>, String> {
    let Some(bytes) = provider.get(&device_head_path(device_id)).await? else {
        return Ok(None);
    };
    let head: WebDavHead = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid WebDAV head for {device_id}: {error}"))?;
    if head.schema_version != 1 || head.device_id != device_id {
        return Err(format!("Invalid WebDAV head identity for {device_id}"));
    }
    Ok(Some(head))
}

#[cfg(test)]
async fn upload_attachment(
    provider: &dyn SyncProvider,
    attachments_dir: &Path,
    attachment: &AttachmentRecord,
) -> Result<(), String> {
    let bytes = std::fs::read(attachments_dir.join(&attachment.relative_path))
        .map_err(|error| format!("Failed to read attachment {}: {error}", attachment.id))?;
    if bytes.len() < ATTACHMENT_CHUNK_THRESHOLD {
        return provider
            .put(
                &format!("attachments/{}", attachment.id),
                bytes,
                &attachment.mime_type,
            )
            .await;
    }

    for (index, chunk) in bytes.chunks(ATTACHMENT_CHUNK_SIZE).enumerate() {
        provider
            .put(
                &attachment_chunk_path(&attachment.id, index),
                chunk.to_vec(),
                "application/octet-stream",
            )
            .await?;
    }
    let manifest = AttachmentChunkManifest {
        schema_version: 1,
        id: attachment.id.clone(),
        size: bytes.len(),
        chunk_size: ATTACHMENT_CHUNK_SIZE,
        chunks: bytes.len().div_ceil(ATTACHMENT_CHUNK_SIZE),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
    };
    provider
        .put(
            &attachment_manifest_path(&attachment.id),
            serde_json::to_vec(&manifest).map_err(|error| error.to_string())?,
            "application/json",
        )
        .await
}

pub(super) fn build_state_envelope(
    db: &Database,
    entity_type: &str,
    entity_id: &str,
    device_id: &str,
) -> Result<SyncEnvelope, String> {
    let causal_version = db
        .ensure_local_causal_version(entity_type, entity_id, device_id)
        .map_err(|e| e.to_string())?;

    let mut operation = "upsert".to_string();
    let mut note = None;
    let mut attachment = None;
    let mut clipboard = None;
    let mut tag = None;
    let mut note_tag = None;
    let mut changed_at = chrono::Utc::now().to_rfc3339();

    match entity_type {
        "note" => {
            note = db.get_note_for_sync(entity_id).map_err(|e| e.to_string())?;
            if let Some(ref n) = note {
                changed_at = n.updated_at.clone();
            }
            if note.as_ref().is_some_and(|n| n.is_deleted) || note.is_none() {
                operation = "delete".to_string();
                note = None;
            }
        }
        "attachment" => {
            attachment = db.get_attachment(entity_id).map_err(|e| e.to_string())?;
            if let Some(ref a) = attachment {
                changed_at = a.created_at.clone();
            }
            if attachment.is_none() {
                operation = "delete".to_string();
            }
        }
        "clipboard" => {
            clipboard = db
                .get_clipboard_item_for_sync(entity_id)
                .map_err(|e| e.to_string())?;
            if let Some(ref c) = clipboard {
                changed_at = c.updated_at.clone();
            }
            if clipboard.as_ref().is_some_and(|c| c.is_deleted) || clipboard.is_none() {
                operation = "delete".to_string();
                clipboard = None;
            }
        }
        "tag" => {
            tag = db.get_tag_for_sync(entity_id).map_err(|e| e.to_string())?;
            if let Some(ref value) = tag {
                changed_at = value.updated_at.clone();
            }
            if tag.as_ref().is_some_and(|value| value.is_deleted) || tag.is_none() {
                operation = "delete".to_string();
                tag = None;
            }
        }
        "note_tag" => {
            note_tag = db
                .get_note_tag_for_sync(entity_id)
                .map_err(|e| e.to_string())?;
            if let Some(ref value) = note_tag {
                changed_at = value.created_at.clone();
            }
            if note_tag.is_none() {
                operation = "delete".to_string();
            }
        }
        _ => {}
    }

    Ok(SyncEnvelope {
        schema_version: 2,
        device_id: device_id.to_string(),
        seq: 0,
        entity_type: entity_type.to_string(),
        entity_id: entity_id.to_string(),
        operation,
        changed_at,
        causal_version: Some(causal_version),
        yjs_update: None,
        note,
        attachment,
        clipboard,
        tag,
        note_tag,
    })
}

/// Build an envelope for cloud sync (per-change, retains seq).
pub(super) fn build_envelope(
    db: &Database,
    change: &SyncChange,
    device_id: &str,
) -> Result<SyncEnvelope, String> {
    let causal_version = db
        .ensure_local_causal_version(&change.entity_type, &change.entity_id, device_id)
        .map_err(|e| e.to_string())?;

    let mut note = None;
    let mut attachment = None;
    let mut clipboard = None;
    let mut tag = None;
    let mut note_tag = None;

    match change.entity_type.as_str() {
        "note" => {
            note = db
                .get_note_for_sync(&change.entity_id)
                .map_err(|e| e.to_string())?;
        }
        "attachment" => {
            attachment = db
                .get_attachment(&change.entity_id)
                .map_err(|e| e.to_string())?;
        }
        "clipboard" => {
            clipboard = db
                .get_clipboard_item_for_sync(&change.entity_id)
                .map_err(|e| e.to_string())?;
        }
        "tag" => {
            tag = db
                .get_tag_for_sync(&change.entity_id)
                .map_err(|e| e.to_string())?;
        }
        "note_tag" => {
            note_tag = db
                .get_note_tag_for_sync(&change.entity_id)
                .map_err(|e| e.to_string())?;
        }
        _ => {}
    }

    Ok(SyncEnvelope {
        schema_version: 2,
        device_id: device_id.to_string(),
        seq: change.seq,
        entity_type: change.entity_type.clone(),
        entity_id: change.entity_id.clone(),
        operation: change.operation.clone(),
        changed_at: change.changed_at.clone(),
        causal_version: Some(causal_version),
        yjs_update: None,
        note,
        attachment,
        clipboard,
        tag,
        note_tag,
    })
}
