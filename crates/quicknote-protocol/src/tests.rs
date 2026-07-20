use super::*;

#[test]
fn causal_relation_detects_concurrent_versions() {
    let left = CausalVersion {
        counters: BTreeMap::from([("a".into(), 1)]),
        origin: "a".into(),
    };
    let right = CausalVersion {
        counters: BTreeMap::from([("b".into(), 1)]),
        origin: "b".into(),
    };
    assert_eq!(left.relation(&right), CausalRelation::Concurrent);
}

#[test]
fn yjs_head_defaults_to_schema_three_empty_cursors() {
    let head = YjsHead::new("note-a", "device-a");
    assert_eq!(head.schema_version, 3);
    assert_eq!(head.note_id, "note-a");
    assert_eq!(head.snapshot, 0);
    assert_eq!(head.latest_update, 0);
    assert_eq!(head.state_version, 0);
    assert_eq!(head.writer_device, "device-a");
}
