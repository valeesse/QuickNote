use super::*;

pub(super) fn validate_envelope(envelope: &SyncEnvelope) -> Result<(), AppError> {
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
        "note" | "attachment" | "clipboard" | "tag" | "note_tag"
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
        ("clipboard", "upsert") => {
            envelope.clipboard.as_ref().map(|item| item.id.as_str())
                == Some(envelope.entity_id.as_str())
        }
        ("tag", "upsert") => {
            envelope.tag.as_ref().map(|item| item.id.as_str()) == Some(envelope.entity_id.as_str())
        }
        ("note_tag", "upsert") => {
            envelope.note_tag.as_ref().map(|item| item.id.as_str())
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
