use super::*;
#[test]
fn note_mutation_and_outbox_commit_together() {
    let (_dir, db) = database();
    let note = db.create_note("<p>标题</p><p>正文</p>").unwrap();
    db.update_note(&note.id, "<p>新标题</p><p>新正文</p>")
        .unwrap();

    let loaded = db.get_note(&note.id).unwrap().unwrap();
    assert_eq!(loaded.title, "新标题");
    assert_eq!(db.list_notes().unwrap()[0].preview, "新正文");
    assert!(db.list_pending_changes(10).unwrap().len() >= 2);
}

#[test]
fn removing_the_last_note_tag_deletes_the_tag() {
    let (_dir, db) = database();
    let note = db.create_note("<p>Tagged note</p>").unwrap();
    db.set_note_tags(&note.id, &["work".to_string()]).unwrap();
    let tag_id = db.list_tags().unwrap()[0].id.clone();

    db.set_note_tags(&note.id, &[]).unwrap();

    assert!(db.list_tags().unwrap().is_empty());
    assert!(db.get_tag_for_sync(&tag_id).unwrap().unwrap().is_deleted);
    assert!(db
        .list_pending_changes(100)
        .unwrap()
        .iter()
        .any(|change| change.entity_type == "tag"
            && change.entity_id == tag_id
            && change.operation == "delete"));
}

#[test]
fn deleting_the_last_tagged_note_deletes_and_restore_revives_the_tag() {
    let (_dir, db) = database();
    let note = db.create_note("<p>Tagged note</p>").unwrap();
    db.set_note_tags(&note.id, &["work".to_string()]).unwrap();

    db.delete_note(&note.id).unwrap();
    assert!(db.list_tags().unwrap().is_empty());

    db.restore_note(&note.id).unwrap();
    let tags = db.list_tags().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "work");
    assert_eq!(tags[0].note_count, 1);
    assert_eq!(db.get_note(&note.id).unwrap().unwrap().tags, vec!["work"]);
}

#[test]
fn deleting_a_note_keeps_tags_used_by_other_notes() {
    let (_dir, db) = database();
    let first = db.create_note("<p>First</p>").unwrap();
    let second = db.create_note("<p>Second</p>").unwrap();
    db.set_note_tags(&first.id, &["shared".to_string()])
        .unwrap();
    db.set_note_tags(&second.id, &["shared".to_string()])
        .unwrap();

    db.delete_note(&first.id).unwrap();

    let tags = db.list_tags().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].note_count, 1);
}

#[test]
fn invalid_version_restore_has_no_side_effect() {
    let (_dir, db) = database();
    let note = db.create_note("<p>标题</p><p>正文</p>").unwrap();
    let before = db.get_note_versions(&note.id).unwrap().len();

    assert!(db
        .restore_note_version(&note.id, i64::MAX)
        .unwrap()
        .is_none());
    assert_eq!(db.get_note_versions(&note.id).unwrap().len(), before);
}

#[test]
fn remote_change_preserves_dirty_local_content_as_conflict_copy() {
    let (_dir, db) = database();
    let local = db.create_note("<p>本地标题</p><p>本地正文</p>").unwrap();
    let remote = Note {
        id: local.id.clone(),
        title: "远端标题".to_string(),
        content: "<p>远端标题</p><p>远端正文</p>".to_string(),
        yjs_state: None,
        yjs_state_version: 0,
        is_pinned: false,
        sort_order: 0,
        created_at: local.created_at,
        updated_at: "9999-12-31T23:59:59Z".to_string(),
        version: 2,
        is_deleted: false,
        tags: Vec::new(),
    };

    let remote_version = CausalVersion::legacy("device-z", 1);
    let (changed, conflict) = db
        .apply_remote_note(&remote, &remote_version, "device-a")
        .unwrap();
    assert!(changed);
    assert!(conflict);
    let notes = db.list_notes().unwrap();
    assert_eq!(notes.len(), 2);
    assert!(notes.iter().any(|note| note.title.contains("冲突副本")));
}

#[test]
fn causal_versions_detect_dominance_and_concurrency() {
    let base = CausalVersion::legacy("seed", 1);
    let mut left = base.clone();
    left.increment("device-a");
    let mut right = base.clone();
    right.increment("device-b");
    assert_eq!(left.relation(&base), CausalRelation::Dominates);
    assert_eq!(base.relation(&left), CausalRelation::Dominated);
    assert_eq!(left.relation(&right), CausalRelation::Concurrent);
}

#[test]
fn concurrent_edits_converge_without_using_wall_clock() {
    let (_dir_a, db_a) = database();
    let (_dir_b, db_b) = database();
    let seed_version = CausalVersion::legacy("seed", 1);
    let seed = Note {
        id: "shared-note".to_string(),
        title: "Seed".to_string(),
        content: "<p>Seed</p>".to_string(),
        yjs_state: None,
        yjs_state_version: 0,
        is_pinned: false,
        sort_order: 0,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        version: 1,
        is_deleted: false,
        tags: Vec::new(),
    };
    db_a.apply_remote_note(&seed, &seed_version, "device-a")
        .unwrap();
    db_b.apply_remote_note(&seed, &seed_version, "device-b")
        .unwrap();

    db_a.update_note(&seed.id, "<p>Edit A</p>").unwrap();
    db_b.update_note(&seed.id, "<p>Edit B</p>").unwrap();
    let version_a = db_a
        .ensure_local_causal_version("note", &seed.id, "device-a")
        .unwrap();
    let version_b = db_b
        .ensure_local_causal_version("note", &seed.id, "device-b")
        .unwrap();
    let mut note_a = db_a.get_note(&seed.id).unwrap().unwrap();
    let mut note_b = db_b.get_note(&seed.id).unwrap().unwrap();
    note_a.updated_at = "9999-12-31T23:59:59Z".to_string();
    note_b.updated_at = "1900-01-01T00:00:00Z".to_string();

    let (_, conflict_a) = db_a
        .apply_remote_note(&note_b, &version_b, "device-a")
        .unwrap();
    let (_, conflict_b) = db_b
        .apply_remote_note(&note_a, &version_a, "device-b")
        .unwrap();
    assert!(conflict_a && conflict_b);
    assert_eq!(
        db_a.get_note(&seed.id).unwrap().unwrap().content,
        "<p>Edit B</p>"
    );
    assert_eq!(
        db_b.get_note(&seed.id).unwrap().unwrap().content,
        "<p>Edit B</p>"
    );
    assert_eq!(db_a.list_notes().unwrap().len(), 2);
    assert_eq!(db_b.list_notes().unwrap().len(), 2);
}

#[test]
fn concurrent_delete_and_edit_converge_and_preserve_the_edit() {
    let (_dir_a, db_a) = database();
    let (_dir_b, db_b) = database();
    let seed_version = CausalVersion::legacy("seed", 1);
    let seed = Note {
        id: "delete-race".to_string(),
        title: "Seed".to_string(),
        content: "<p>Seed</p>".to_string(),
        yjs_state: None,
        yjs_state_version: 0,
        is_pinned: false,
        sort_order: 0,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        version: 1,
        is_deleted: false,
        tags: Vec::new(),
    };
    db_a.apply_remote_note(&seed, &seed_version, "device-a")
        .unwrap();
    db_b.apply_remote_note(&seed, &seed_version, "device-b")
        .unwrap();
    db_a.update_note(&seed.id, "<p>Keep this edit</p>").unwrap();
    db_b.delete_note(&seed.id).unwrap();
    let version_a = db_a
        .ensure_local_causal_version("note", &seed.id, "device-a")
        .unwrap();
    let version_b = db_b
        .ensure_local_causal_version("note", &seed.id, "device-b")
        .unwrap();
    let mut edited = db_a.get_note(&seed.id).unwrap().unwrap();
    edited.updated_at = "9999-12-31T23:59:59Z".to_string();

    let (_, conflict_a) = db_a
        .apply_remote_delete(&seed.id, "1900-01-01T00:00:00Z", &version_b, "device-a")
        .unwrap();
    let (_, conflict_b) = db_b
        .apply_remote_note(&edited, &version_a, "device-b")
        .unwrap();
    assert!(conflict_a && conflict_b);
    assert!(
        db_a.get_note_for_sync(&seed.id)
            .unwrap()
            .unwrap()
            .is_deleted
    );
    assert!(
        db_b.get_note_for_sync(&seed.id)
            .unwrap()
            .unwrap()
            .is_deleted
    );
    assert!(db_a
        .list_notes()
        .unwrap()
        .iter()
        .any(|note| note.title.contains("冲突副本")));
    assert!(db_b
        .list_notes()
        .unwrap()
        .iter()
        .any(|note| note.title.contains("冲突副本")));
}
