use super::*;
#[test]
fn clipboard_capture_deduplicates_classifies_and_queues_sync() {
    let (_dir, db) = database();
    let first = db
        .capture_clipboard("https://example.com/path\r\n", "device-a")
        .unwrap();
    let second = db
        .capture_clipboard("https://example.com/path\n", "device-a")
        .unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.kind, "link");
    assert_eq!(second.capture_count, 2);
    assert_eq!(db.list_clipboard_items("example", 10).unwrap().len(), 1);
    assert!(db
        .list_pending_changes(20)
        .unwrap()
        .iter()
        .any(|change| change.entity_type == "clipboard"));
}

#[test]
fn duplicate_clipboard_capture_moves_item_to_the_front_of_its_group() {
    let (_dir, db) = database();
    db.capture_clipboard_at("first", "device-a", "2026-01-01T00:00:00Z")
        .unwrap();
    db.capture_clipboard_at("second", "device-a", "2026-01-01T00:00:10Z")
        .unwrap();
    db.capture_clipboard_at("first", "device-a", "2026-01-01T00:00:20Z")
        .unwrap();

    let items = db.list_clipboard_items("", 10).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].content, "first");
    assert_eq!(items[0].capture_count, 2);
    assert_eq!(items[1].content, "second");
}

#[test]
fn clipboard_retains_only_the_500_most_relevant_items() {
    let (_dir, db) = database();
    for index in 0..501 {
        db.capture_clipboard_at(
            &format!("item-{index:03}"),
            "device-a",
            &format!("2026-01-01T00:{:02}:{:02}Z", index / 60, index % 60),
        )
        .unwrap();
    }

    let items = db.list_clipboard_items("", 500).unwrap();
    assert_eq!(items.len(), 500);
    assert!(items.iter().any(|item| item.content == "item-500"));
    assert!(!items.iter().any(|item| item.content == "item-000"));
}

#[test]
fn clipboard_history_pages_without_repeating_items() {
    let (_dir, db) = database();
    for index in 0..75 {
        db.capture_clipboard_at(
            &format!("page-item-{index:03}"),
            "device-a",
            &format!("2026-01-01T00:{:02}:{:02}Z", index / 60, index % 60),
        )
        .unwrap();
    }

    let first = db.list_clipboard_items_page("", 50, 0).unwrap();
    let second = db.list_clipboard_items_page("", 50, 50).unwrap();
    assert_eq!(first.len(), 50);
    assert_eq!(second.len(), 25);
    assert!(first
        .iter()
        .all(|item| !second.iter().any(|next| next.id == item.id)));
}

#[test]
fn clipboard_image_attachments_are_not_orphaned() {
    let (_dir, db) = database();
    let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    db.register_attachment(id, &format!("{id}.webp"), "image/webp", 12)
        .unwrap();
    db.capture_clipboard(
        &format!(r#"<img src="attachment://{id}" alt="剪贴板图片">"#),
        "device-a",
    )
    .unwrap();

    assert!(db.orphan_attachments().unwrap().is_empty());
}

#[test]
fn deleting_clipboard_image_queues_its_attachment_for_gc() {
    let (_dir, db) = database();
    let attachment_id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    db.register_attachment(
        attachment_id,
        &format!("{attachment_id}.png"),
        "image/png",
        12,
    )
    .unwrap();
    let item = db
        .capture_clipboard(
            &format!(r#"<img src="attachment://{attachment_id}">"#),
            "device-a",
        )
        .unwrap();

    assert!(db.clipboard_attachment_gc_candidates().unwrap().is_empty());
    db.delete_clipboard_item(&item.id).unwrap();
    assert_eq!(
        db.clipboard_attachment_gc_candidates().unwrap()[0].id,
        attachment_id
    );
}

#[test]
fn clipboard_classifies_image_first_mixed_content_as_rich() {
    let (_dir, db) = database();
    let mixed = db
        .capture_clipboard(
            r#"<img src="https://example.com/image.png"><p>图片说明</p>"#,
            "device-a",
        )
        .unwrap();
    let image = db
        .capture_clipboard(r#"<img src="https://example.com/image.png">"#, "device-a")
        .unwrap();

    assert_eq!(mixed.kind, "rich");
    assert_eq!(mixed.preview, "图片说明");
    assert_eq!(image.kind, "image");
}

#[test]
fn bootstrap_can_requeue_historical_data_for_a_new_cloud_scope() {
    let (_dir, db) = database();
    let note = db.create_note("<p>历史数据</p>").unwrap();

    for change in db.list_pending_changes(20).unwrap() {
        db.mark_change_synced(change.seq).unwrap();
    }
    assert!(db.list_pending_changes(20).unwrap().is_empty());

    db.ensure_sync_bootstrap("cloud:https://cloud.test:user@example.com")
        .unwrap();

    let pending = db.list_pending_changes(20).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].entity_type, "note");
    assert_eq!(pending[0].entity_id, note.id);

    db.ensure_sync_bootstrap("cloud:https://cloud.test:user@example.com")
        .unwrap();
    assert_eq!(db.list_pending_changes(20).unwrap().len(), 1);
}

#[test]
fn concurrent_clipboard_metadata_converges_by_causal_version() {
    let (_dir_a, db_a) = database();
    let (_dir_b, db_b) = database();
    let item_a = db_a.capture_clipboard("shared", "device-a").unwrap();
    let base_version = db_a
        .ensure_local_causal_version("clipboard", &item_a.id, "seed")
        .unwrap();
    db_b.apply_remote_clipboard(&item_a, &base_version, "device-b")
        .unwrap();
    db_a.toggle_clipboard_pin(&item_a.id).unwrap();
    db_b.toggle_clipboard_pin(&item_a.id).unwrap();
    let version_a = db_a
        .ensure_local_causal_version("clipboard", &item_a.id, "device-a")
        .unwrap();
    let version_b = db_b
        .ensure_local_causal_version("clipboard", &item_a.id, "device-b")
        .unwrap();
    let item_a = db_a
        .get_clipboard_item_for_sync(&item_a.id)
        .unwrap()
        .unwrap();
    let item_b = db_b
        .get_clipboard_item_for_sync(&item_a.id)
        .unwrap()
        .unwrap();
    db_a.apply_remote_clipboard(&item_b, &version_b, "device-a")
        .unwrap();
    db_b.apply_remote_clipboard(&item_a, &version_a, "device-b")
        .unwrap();
    assert_eq!(
        db_a.get_clipboard_item_for_sync(&item_a.id)
            .unwrap()
            .unwrap()
            .is_pinned,
        db_b.get_clipboard_item_for_sync(&item_a.id)
            .unwrap()
            .unwrap()
            .is_pinned
    );
}
