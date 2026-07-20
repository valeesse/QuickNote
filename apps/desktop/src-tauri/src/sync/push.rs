use super::*;

pub(super) fn state_file_path(device_id: &str, entity_type: &str, entity_id: &str) -> String {
    format!("state/{device_id}/{entity_type}/{entity_id}.json")
}

pub(super) async fn push_state(
    provider: &dyn SyncProvider,
    db: &Database,
    attachments_dir: &Path,
    config: &SyncConfig,
) -> Result<usize, String> {
    let changes = db.list_pending_changes(500).map_err(|e| e.to_string())?;

    // Deduplicate: collect unique (entity_type, entity_id) pairs
    let mut seen = std::collections::HashSet::new();
    let mut entities: Vec<(String, String)> = Vec::new();
    for change in &changes {
        let key = (change.entity_type.clone(), change.entity_id.clone());
        if seen.insert(key.clone()) {
            entities.push(key);
        }
    }

    let mut pushed = 0;
    for (entity_type, entity_id) in &entities {
        let envelope = build_state_envelope(db, entity_type, entity_id, &config.device_id)?;

        if let Some(attachment) = &envelope.attachment {
            let bytes = std::fs::read(attachments_dir.join(&attachment.relative_path))
                .map_err(|e| format!("Failed to read attachment {}: {e}", attachment.id))?;
            provider
                .put(
                    &format!("attachments/{}", attachment.id),
                    bytes,
                    &attachment.mime_type,
                )
                .await?;
        }

        let body = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;
        let path = state_file_path(&config.device_id, entity_type, entity_id);
        provider.put(&path, body, "application/json").await?;
        db.mark_entity_changes_synced(entity_type, entity_id)
            .map_err(|e| e.to_string())?;
        pushed += 1;
    }
    Ok(pushed)
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
