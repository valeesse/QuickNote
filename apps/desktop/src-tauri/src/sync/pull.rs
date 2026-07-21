use super::*;

#[cfg(test)]
pub(super) async fn pull_state(
    provider: &dyn SyncProvider,
    db: &Database,
    attachments_dir: &Path,
    config: &SyncConfig,
) -> Result<(usize, usize), String> {
    let mut pulled = 0;
    let mut conflicts = 0;

    pulled += repair_missing_attachments(provider, db, attachments_dir).await?;

    // Protocol v3: fetch only generations newer than the durable per-device cursor.
    let mut batch_devices = std::collections::HashSet::new();
    for file in provider.list("device-heads").await? {
        let Some(device_id) = file.strip_suffix(".json") else {
            continue;
        };
        if device_id == config.device_id || !is_safe_path_segment(device_id) {
            continue;
        }
        let Some(head) = read_device_head(provider, device_id).await? else {
            continue;
        };
        batch_devices.insert(device_id.to_string());
        let cursor_scope = format!("webdav-v3:{}", config.endpoint);
        let cursor = db
            .get_sync_cursor_value(&cursor_scope, device_id)
            .map_err(|error| error.to_string())?
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        for generation in (cursor + 1)..=head.generation {
            let path = batch_path(device_id, generation);
            let bytes = provider
                .get(&path)
                .await?
                .ok_or_else(|| format!("Missing WebDAV change batch {path}"))?;
            let batch: WebDavBatch = gunzip_json(&bytes)
                .map_err(|error| format!("Invalid WebDAV change batch {path}: {error}"))?;
            if batch.schema_version != 1
                || batch.device_id != device_id
                || batch.generation != generation
            {
                return Err(format!("Invalid WebDAV change batch identity at {path}"));
            }
            for envelope in &batch.envelopes {
                validate_envelope(envelope, device_id)?;
                let (changed, conflict) =
                    apply_envelope(provider, db, attachments_dir, envelope, &config.device_id)
                        .await?;
                if changed {
                    pulled += 1;
                }
                if conflict {
                    conflicts += 1;
                }
            }
            db.set_sync_cursor_value(&cursor_scope, device_id, &generation.to_string())
                .map_err(|error| error.to_string())?;
        }
    }

    // Legacy state protocol remains readable for devices that have not migrated yet.
    let device_ids = provider.list("state").await?;
    for device_id in &device_ids {
        if device_id == &config.device_id
            || !is_safe_path_segment(device_id)
            || batch_devices.contains(device_id)
        {
            continue;
        }
        let revision = provider
            .get(&revision_file_path(device_id))
            .await?
            .and_then(|body| String::from_utf8(body).ok());
        let cursor_scope = format!("webdav:{}", config.endpoint);
        let local_attachments_available = db
            .list_attachments_for_sync()
            .map_err(|e| e.to_string())?
            .iter()
            .all(|record| attachments_dir.join(&record.relative_path).exists());
        if local_attachments_available
            && revision.as_ref().is_some_and(|value| {
                db.get_sync_cursor_value(&cursor_scope, device_id)
                    .ok()
                    .flatten()
                    .as_ref()
                    == Some(value)
            })
        {
            continue;
        }
        // List entity types under state/{device_id}/
        let entity_types = provider.list(&format!("state/{device_id}")).await?;
        for entity_type in &entity_types {
            if entity_type == "meta" || !is_safe_path_segment(entity_type) {
                continue;
            }
            // List entity files under state/{device_id}/{entity_type}/
            let files = provider
                .list(&format!("state/{device_id}/{entity_type}"))
                .await?;
            for file in &files {
                let Some(entity_id) = file.strip_suffix(".json") else {
                    continue;
                };
                if entity_id.is_empty() || !is_safe_path_segment(entity_id) {
                    continue;
                }
                let path = format!("state/{device_id}/{entity_type}/{file}");
                let Some(body) = provider.get(&path).await? else {
                    continue;
                };
                let envelope: SyncEnvelope = match serde_json::from_slice(&body) {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("Skipping invalid state file {path}: {e}");
                        continue;
                    }
                };
                if let Err(e) = validate_envelope(&envelope, device_id) {
                    eprintln!("Skipping invalid envelope {path}: {e}");
                    continue;
                }

                // Compare causal versions
                let legacy_fallback = CausalVersion::legacy(&envelope.device_id, envelope.seq);
                let remote_version = envelope.causal_version.as_ref().unwrap_or(&legacy_fallback);
                let local_version = db
                    .get_entity_causal_version(entity_type, entity_id)
                    .map_err(|e| e.to_string())?;

                let attachment_missing = envelope.entity_type == "attachment"
                    && envelope.attachment.as_ref().is_some_and(|record| {
                        !local_attachment_available(db, attachments_dir, record)
                    });
                let should_apply = attachment_missing
                    || match &local_version {
                        None => true, // no local version → apply
                        Some(local) => match remote_version.relation(local) {
                            CausalRelation::Dominates | CausalRelation::Concurrent => true,
                            CausalRelation::Equal | CausalRelation::Dominated => false,
                        },
                    };

                if !should_apply {
                    continue;
                }

                let (changed, conflict) =
                    apply_envelope(provider, db, attachments_dir, &envelope, &config.device_id)
                        .await?;
                if changed {
                    pulled += 1;
                }
                if conflict {
                    conflicts += 1;
                }
            }
        }
        if let Some(revision) = revision {
            db.set_sync_cursor_value(&cursor_scope, device_id, &revision)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok((pulled, conflicts))
}

#[cfg(test)]
pub(super) fn local_attachment_available(
    db: &Database,
    attachments_dir: &Path,
    record: &AttachmentRecord,
) -> bool {
    db.get_attachment(&record.id)
        .ok()
        .flatten()
        .is_some_and(|local| attachments_dir.join(local.relative_path).exists())
}

pub(super) fn is_safe_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

pub(super) async fn apply_envelope(
    provider: &dyn SyncProvider,
    db: &Database,
    attachments_dir: &Path,
    envelope: &SyncEnvelope,
    local_device_id: &str,
) -> Result<(bool, bool), String> {
    let causal_version = envelope
        .causal_version
        .clone()
        .unwrap_or_else(|| CausalVersion::legacy(&envelope.device_id, envelope.seq));
    match envelope.entity_type.as_str() {
        "note" if envelope.operation == "delete" => db
            .apply_remote_delete(
                &envelope.entity_id,
                &envelope.changed_at,
                &causal_version,
                local_device_id,
            )
            .map_err(|e| e.to_string()),
        "note" => {
            let Some(note) = &envelope.note else {
                return Ok((false, false));
            };
            db.apply_remote_note(note, &causal_version, local_device_id)
                .map_err(|e| e.to_string())
        }
        "attachment" => {
            let Some(record) = &envelope.attachment else {
                return Ok((false, false));
            };
            validate_attachment_path(record)?;
            let local_path = attachments_dir.join(&record.relative_path);
            if !local_path.exists() {
                let bytes = download_attachment(provider, record).await?;
                validate_attachment(record, &bytes)?;
                std::fs::create_dir_all(attachments_dir)
                    .map_err(|e| format!("Failed to create attachment directory: {e}"))?;
                let temporary_path = attachments_dir.join(format!(".{}.part", record.id));
                std::fs::write(&temporary_path, bytes)
                    .map_err(|e| format!("Failed to write attachment {}: {e}", record.id))?;
                if let Err(error) = std::fs::rename(&temporary_path, &local_path) {
                    let _ = std::fs::remove_file(&temporary_path);
                    return Err(format!(
                        "Failed to finalize attachment {}: {error}",
                        record.id
                    ));
                }
            }
            db.register_remote_attachment(record)
                .map_err(|e| e.to_string())?;
            Ok((true, false))
        }
        "clipboard" if envelope.operation == "delete" => db
            .apply_remote_clipboard_delete(&envelope.entity_id, &causal_version, local_device_id)
            .map(|changed| (changed, false))
            .map_err(|e| e.to_string()),
        "clipboard" => {
            let Some(item) = &envelope.clipboard else {
                return Ok((false, false));
            };
            db.apply_remote_clipboard(item, &causal_version, local_device_id)
                .map(|changed| (changed, false))
                .map_err(|e| e.to_string())
        }
        "tag" => {
            let Some(tag) = &envelope.tag else {
                return Ok((false, false));
            };
            db.apply_remote_tag(tag, &causal_version, local_device_id)
                .map(|changed| (changed, false))
                .map_err(|e| e.to_string())
        }
        "note_tag" if envelope.operation == "delete" => db
            .apply_remote_note_tag_delete(&envelope.entity_id, &causal_version, local_device_id)
            .map(|changed| (changed, false))
            .map_err(|e| e.to_string()),
        "note_tag" => {
            let Some(note_tag) = &envelope.note_tag else {
                return Ok((false, false));
            };
            db.apply_remote_note_tag(note_tag, &causal_version, local_device_id)
                .map(|changed| (changed, false))
                .map_err(|e| e.to_string())
        }
        _ => Ok((false, false)),
    }
}

#[cfg(test)]
async fn repair_missing_attachments(
    provider: &dyn SyncProvider,
    db: &Database,
    attachments_dir: &Path,
) -> Result<usize, String> {
    let mut repaired = 0;
    for record in db
        .list_attachments_for_sync()
        .map_err(|error| error.to_string())?
    {
        if attachments_dir.join(&record.relative_path).exists() {
            continue;
        }
        let bytes = download_attachment(provider, &record).await?;
        validate_attachment(&record, &bytes)?;
        std::fs::create_dir_all(attachments_dir)
            .map_err(|error| format!("Failed to create attachment directory: {error}"))?;
        let temporary_path = attachments_dir.join(format!(".{}.sync-part", record.id));
        std::fs::write(&temporary_path, &bytes)
            .map_err(|error| format!("Failed to stage attachment {}: {error}", record.id))?;
        std::fs::rename(&temporary_path, attachments_dir.join(&record.relative_path))
            .map_err(|error| format!("Failed to install attachment {}: {error}", record.id))?;
        repaired += 1;
    }
    Ok(repaired)
}

async fn download_attachment(
    provider: &dyn SyncProvider,
    record: &AttachmentRecord,
) -> Result<Vec<u8>, String> {
    if let Some(bytes) = v4::download_attachment(provider, record).await? {
        return Ok(bytes);
    }
    if let Some(bytes) = provider.get(&format!("attachments/{}", record.id)).await? {
        return Ok(bytes);
    }
    let manifest_path = attachment_manifest_path(&record.id);
    let manifest_bytes = provider
        .get(&manifest_path)
        .await?
        .ok_or_else(|| format!("Remote attachment {} is missing", record.id))?;
    let manifest: AttachmentChunkManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("Invalid attachment manifest {manifest_path}: {error}"))?;
    if manifest.schema_version != 1
        || manifest.id != record.id
        || manifest.size != record.size as usize
        || manifest.sha256 != record.id
        || manifest.chunk_size == 0
        || manifest.chunks == 0
    {
        return Err(format!("Invalid attachment manifest for {}", record.id));
    }
    let mut bytes = Vec::with_capacity(manifest.size);
    for index in 0..manifest.chunks {
        let path = attachment_chunk_path(&record.id, index);
        let chunk = provider
            .get(&path)
            .await?
            .ok_or_else(|| format!("Missing attachment chunk {path}"))?;
        if index + 1 < manifest.chunks && chunk.len() != manifest.chunk_size {
            return Err(format!("Invalid attachment chunk size at {path}"));
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() != manifest.size || format!("{:x}", Sha256::digest(&bytes)) != manifest.sha256 {
        return Err(format!(
            "Attachment {} failed integrity validation",
            record.id
        ));
    }
    Ok(bytes)
}

pub(super) fn validate_envelope(
    envelope: &SyncEnvelope,
    expected_device: &str,
) -> Result<(), String> {
    if !matches!(envelope.schema_version, 1 | 2) {
        return Err(format!(
            "unsupported schema version {}",
            envelope.schema_version
        ));
    }
    if envelope.schema_version == 2 && envelope.causal_version.is_none() {
        return Err("schema v2 change is missing its causal version".to_string());
    }
    if envelope.device_id != expected_device {
        return Err("device does not match expected source".to_string());
    }
    if !matches!(
        envelope.entity_type.as_str(),
        "note" | "attachment" | "clipboard" | "tag" | "note_tag"
    ) {
        return Err(format!("unsupported entity type {}", envelope.entity_type));
    }
    if !matches!(envelope.operation.as_str(), "upsert" | "delete") {
        return Err(format!("unsupported operation {}", envelope.operation));
    }
    if envelope.entity_type == "note"
        && envelope.operation == "upsert"
        && envelope.note.as_ref().map(|note| note.id.as_str()) != Some(envelope.entity_id.as_str())
    {
        return Err("note payload does not match its entity ID".to_string());
    }
    if envelope.entity_type == "attachment"
        && envelope.operation == "upsert"
        && envelope.attachment.as_ref().map(|item| item.id.as_str())
            != Some(envelope.entity_id.as_str())
    {
        return Err("attachment payload does not match its entity ID".to_string());
    }
    if envelope.entity_type == "clipboard"
        && envelope.operation == "upsert"
        && envelope.clipboard.as_ref().map(|item| item.id.as_str())
            != Some(envelope.entity_id.as_str())
    {
        return Err("clipboard payload does not match its entity ID".to_string());
    }
    if envelope.entity_type == "tag"
        && envelope.operation == "upsert"
        && envelope.tag.as_ref().map(|item| item.id.as_str()) != Some(envelope.entity_id.as_str())
    {
        return Err("tag payload does not match its entity ID".to_string());
    }
    if envelope.entity_type == "note_tag"
        && envelope.operation == "upsert"
        && envelope.note_tag.as_ref().map(|item| item.id.as_str())
            != Some(envelope.entity_id.as_str())
    {
        return Err("note_tag payload does not match its entity ID".to_string());
    }
    Ok(())
}

pub(super) fn validate_attachment(record: &AttachmentRecord, bytes: &[u8]) -> Result<(), String> {
    validate_attachment_path(record)?;
    if record.size < 0 || record.size as usize != bytes.len() {
        return Err("Remote attachment size does not match its metadata".to_string());
    }
    let actual_id = format!("{:x}", Sha256::digest(bytes));
    if actual_id != record.id.to_ascii_lowercase() {
        return Err("Remote attachment content hash does not match its ID".to_string());
    }
    Ok(())
}

pub(super) fn validate_attachment_path(record: &AttachmentRecord) -> Result<(), String> {
    if record.id.len() != 64 || !record.id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Remote attachment has an invalid content ID".to_string());
    }
    let mut components = Path::new(&record.relative_path).components();
    let Some(Component::Normal(filename)) = components.next() else {
        return Err("Remote attachment path is invalid".to_string());
    };
    if components.next().is_some() {
        return Err("Remote attachment path must be a single filename".to_string());
    }
    let filename = filename.to_string_lossy();
    if !filename.starts_with(&format!("{}.", record.id)) {
        return Err("Remote attachment filename does not match its content ID".to_string());
    }
    Ok(())
}
