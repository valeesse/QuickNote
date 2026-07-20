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
            yjs_state: None,
            yjs_state_version: 0,
            is_pinned: false,
            sort_order: 0,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            version: 1,
            is_deleted: false,
            tags: Vec::new(),
        }),
        yjs_update: None,
        attachment: None,
        clipboard: None,
        tag: None,
        note_tag: None,
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

#[test]
fn accepts_clipboard_delete_without_payload() {
    let mut value = envelope();
    value.entity_type = "clipboard".into();
    value.entity_id = "clipboard-a".into();
    value.operation = "delete".into();
    value.note = None;
    assert!(validate_envelope(&value).is_ok());
}
