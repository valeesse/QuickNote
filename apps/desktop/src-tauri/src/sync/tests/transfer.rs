use super::*;

#[tokio::test]
async fn clipboard_item_round_trips_between_devices() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    let item = db_a
        .capture_clipboard("https://example.com/cross-device", "device-a")
        .unwrap();

    assert_eq!(
        push_state(&provider, &db_a, dir_a.path(), &config("device-a"))
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        pull_state(&provider, &db_b, dir_b.path(), &config("device-b"))
            .await
            .unwrap(),
        (1, 0)
    );
    assert_eq!(
        db_b.get_clipboard_item(&item.id).unwrap().unwrap().content,
        item.content
    );

    provider.objects.lock().unwrap().insert(
        state_file_path("device-a", "clipboard", &item.id),
        b"invalid state that must not be fetched again".to_vec(),
    );
    assert_eq!(
        pull_state(&provider, &db_b, dir_b.path(), &config("device-b"))
            .await
            .unwrap(),
        (0, 0)
    );
}

#[tokio::test]
async fn clipboard_item_reappears_remotely_when_same_content_is_copied_after_clear() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    let config_a = config("device-a");
    let config_b = config("device-b");

    let original = db_a.capture_clipboard("copy me again", "device-a").unwrap();
    push_state(&provider, &db_a, dir_a.path(), &config_a)
        .await
        .unwrap();
    pull_state(&provider, &db_b, dir_b.path(), &config_b)
        .await
        .unwrap();
    assert!(db_b.get_clipboard_item(&original.id).unwrap().is_some());

    assert_eq!(db_a.clear_clipboard_items().unwrap(), 1);
    push_state(&provider, &db_a, dir_a.path(), &config_a)
        .await
        .unwrap();
    pull_state(&provider, &db_b, dir_b.path(), &config_b)
        .await
        .unwrap();
    assert!(db_b.get_clipboard_item(&original.id).unwrap().is_none());

    let copied_again = db_a.capture_clipboard("copy me again", "device-a").unwrap();
    assert_eq!(copied_again.id, original.id);
    push_state(&provider, &db_a, dir_a.path(), &config_a)
        .await
        .unwrap();
    pull_state(&provider, &db_b, dir_b.path(), &config_b)
        .await
        .unwrap();

    assert_eq!(
        db_b.get_clipboard_item(&original.id)
            .unwrap()
            .unwrap()
            .content,
        "copy me again"
    );
}

#[tokio::test]
async fn one_push_drains_more_than_one_pending_change_batch() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().to_path_buf()).unwrap();
    for index in 0..501 {
        db.create_note(&format!("<p>{index}</p>")).unwrap();
    }
    let provider = MemoryProvider::default();

    assert_eq!(
        push_state(&provider, &db, dir.path(), &config("device-a"))
            .await
            .unwrap(),
        501
    );
    assert!(db.list_pending_changes(1).unwrap().is_empty());
}

#[tokio::test]
async fn large_attachment_uses_resumable_chunks_and_round_trips() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    let bytes = vec![0x5a; ATTACHMENT_CHUNK_THRESHOLD + 17];
    let id = format!("{:x}", Sha256::digest(&bytes));
    let filename = format!("{id}.bin");
    std::fs::write(dir_a.path().join(&filename), &bytes).unwrap();
    db_a.register_attachment(
        &id,
        &filename,
        "application/octet-stream",
        bytes.len() as i64,
    )
    .unwrap();

    push_state(&provider, &db_a, dir_a.path(), &config("device-a"))
        .await
        .unwrap();
    assert!(provider
        .objects
        .lock()
        .unwrap()
        .contains_key(&attachment_manifest_path(&id)));
    assert_eq!(
        provider
            .objects
            .lock()
            .unwrap()
            .keys()
            .filter(|path| path.starts_with(&format!("attachment-chunks/{id}.")))
            .count(),
        5
    );

    pull_state(&provider, &db_b, dir_b.path(), &config("device-b"))
        .await
        .unwrap();
    assert_eq!(std::fs::read(dir_b.path().join(filename)).unwrap(), bytes);
}

#[tokio::test]
async fn v4_large_attachment_is_chunked_round_trips_and_is_garbage_collected() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    let cfg_a = config("device-a");
    let cfg_b = config("device-b");
    let workspace = v4::ensure_workspace(&provider, &cfg_a.device_id)
        .await
        .unwrap();
    let bytes: Vec<u8> = (0..4 * 1024 * 1024 + 17)
        .map(|index| ((index / (1024 * 1024)) + 0x60) as u8)
        .collect();
    let id = format!("{:x}", Sha256::digest(&bytes));
    let filename = format!("{id}.bin");
    std::fs::write(dir_a.path().join(&filename), &bytes).unwrap();
    db_a.register_attachment(
        &id,
        &filename,
        "application/octet-stream",
        bytes.len() as i64,
    )
    .unwrap();

    v4::push(&provider, &db_a, dir_a.path(), &cfg_a, &workspace)
        .await
        .unwrap();
    {
        let objects = provider.objects.lock().unwrap();
        assert_eq!(
            objects
                .keys()
                .filter(|path| path.starts_with("v4/attachment-manifests/"))
                .count(),
            1
        );
        assert_eq!(
            objects
                .keys()
                .filter(|path| path.starts_with("v4/attachment-chunks/"))
                .count(),
            5
        );
        assert!(!objects
            .keys()
            .any(|path| path.starts_with("v4/attachments/")));
    }

    v4::pull(&provider, &db_b, dir_b.path(), &cfg_b, &workspace)
        .await
        .unwrap();
    assert_eq!(std::fs::read(dir_b.path().join(&filename)).unwrap(), bytes);

    db_a.remove_attachment_record(&id).unwrap();
    v4::push(&provider, &db_a, dir_a.path(), &cfg_a, &workspace)
        .await
        .unwrap();
    v4::run_gc(&provider, &workspace, 0).await.unwrap();
    v4::run_gc(&provider, &workspace, 0).await.unwrap();
    let objects = provider.objects.lock().unwrap();
    assert!(!objects
        .keys()
        .any(|path| path.starts_with("v4/attachment-manifests/")));
    assert!(!objects
        .keys()
        .any(|path| path.starts_with("v4/attachment-chunks/")));
}

#[tokio::test]
async fn revision_acknowledgement_loss_keeps_changes_retryable() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    let item = db_a
        .capture_clipboard("survives a lost WebDAV response", "device-a")
        .unwrap();

    provider.fail_put_path_once(&device_head_path("device-a"));
    assert!(
        push_state(&provider, &db_a, dir_a.path(), &config("device-a"))
            .await
            .is_err()
    );
    assert!(!db_a.list_pending_changes(1).unwrap().is_empty());

    assert_eq!(
        push_state(&provider, &db_a, dir_a.path(), &config("device-a"))
            .await
            .unwrap(),
        1
    );
    assert!(db_a.list_pending_changes(1).unwrap().is_empty());
    assert_eq!(
        pull_state(&provider, &db_b, dir_b.path(), &config("device-b"))
            .await
            .unwrap(),
        (1, 0)
    );
    assert_eq!(
        db_b.get_clipboard_item(&item.id).unwrap().unwrap().content,
        item.content
    );
}

#[tokio::test]
async fn transient_slow_list_and_get_failures_eventually_converge() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    provider.set_delay_ms(3);
    let item = db_a
        .capture_clipboard("eventual clipboard", "device-a")
        .unwrap();
    push_state(&provider, &db_a, dir_a.path(), &config("device-a"))
        .await
        .unwrap();

    provider.fail_next_lists(1);
    assert!(
        pull_state(&provider, &db_b, dir_b.path(), &config("device-b"))
            .await
            .is_err()
    );
    provider.fail_next_gets(1);
    assert!(
        pull_state(&provider, &db_b, dir_b.path(), &config("device-b"))
            .await
            .is_err()
    );

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        pull_state(&provider, &db_b, dir_b.path(), &config("device-b")),
    )
    .await
    .expect("slow sync should still complete")
    .unwrap();
    assert_eq!(
        db_b.get_clipboard_item(&item.id).unwrap().unwrap().content,
        item.content
    );
}

#[tokio::test]
async fn remote_revision_wakes_an_idle_second_device() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    let config_a = config("device-a");
    let config_b = config("device-b");

    db_a.capture_clipboard("remote-only change", "device-a")
        .unwrap();
    push_state(&provider, &db_a, dir_a.path(), &config_a)
        .await
        .unwrap();
    assert!(
        service::webdav_has_remote_changes(&provider, &db_b, &config_b)
            .await
            .unwrap()
    );

    pull_state(&provider, &db_b, dir_b.path(), &config_b)
        .await
        .unwrap();
    assert!(
        !service::webdav_has_remote_changes(&provider, &db_b, &config_b)
            .await
            .unwrap()
    );

    db_a.capture_clipboard("a newer remote-only change", "device-a")
        .unwrap();
    push_state(&provider, &db_a, dir_a.path(), &config_a)
        .await
        .unwrap();
    assert!(
        service::webdav_has_remote_changes(&provider, &db_b, &config_b)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn multiple_edits_produce_append_only_generation_batches() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().to_path_buf()).unwrap();
    let note = db.create_note("<p>first</p>").unwrap();
    let provider = MemoryProvider::default();
    let sync_config = config("device-a");

    // First push
    assert_eq!(
        push_state(&provider, &db, dir.path(), &sync_config)
            .await
            .unwrap(),
        1
    );

    // Edit same note again
    db.update_note(&note.id, "<p>second</p>").unwrap();
    assert_eq!(
        push_state(&provider, &db, dir.path(), &sync_config)
            .await
            .unwrap(),
        1
    );

    // Each completed edit is an immutable generation; unchanged state is never rescanned.
    let batches: Vec<String> = provider
        .objects
        .lock()
        .unwrap()
        .keys()
        .filter(|key| key.starts_with("changes/device-a/"))
        .cloned()
        .collect();
    assert_eq!(batches.len(), 2);
    assert_eq!(
        read_device_head(&provider, "device-a")
            .await
            .unwrap()
            .unwrap()
            .generation,
        2
    );
}

#[tokio::test]
async fn pull_skips_dominated_remote_state() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();

    // Device A creates and pushes a note
    let note = db_a.create_note("<p>hello</p>").unwrap();
    assert_eq!(
        push_state(&provider, &db_a, dir_a.path(), &config("device-a"))
            .await
            .unwrap(),
        1
    );

    // Device B pulls it
    assert_eq!(
        pull_state(&provider, &db_b, dir_b.path(), &config("device-b"))
            .await
            .unwrap(),
        (1, 0)
    );

    // Device B edits and pushes
    db_b.update_note(&note.id, "<p>edited by B</p>").unwrap();
    assert_eq!(
        push_state(&provider, &db_b, dir_b.path(), &config("device-b"))
            .await
            .unwrap(),
        1
    );

    // Device A pulls B's edit — should apply (B dominates)
    let (pulled, _) = pull_state(&provider, &db_a, dir_a.path(), &config("device-a"))
        .await
        .unwrap();
    assert_eq!(pulled, 1);

    // Second pull without changes — should skip (equal)
    let (pulled2, _) = pull_state(&provider, &db_a, dir_a.path(), &config("device-a"))
        .await
        .unwrap();
    assert_eq!(pulled2, 0);
}

#[tokio::test]
async fn v4_many_edits_are_bounded_after_two_phase_gc() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    let cfg = config("device-a");
    let workspace = v4::ensure_workspace(&provider, &cfg.device_id)
        .await
        .unwrap();
    let note = db.create_note("<p>0</p>").unwrap();

    v4::push(&provider, &db, dir.path(), &cfg, &workspace)
        .await
        .unwrap();
    for index in 1..=100 {
        db.update_note(&note.id, &format!("<p>{index}</p>"))
            .unwrap();
        v4::push(&provider, &db, dir.path(), &cfg, &workspace)
            .await
            .unwrap();
    }

    v4::run_gc(&provider, &workspace, 0).await.unwrap();
    v4::run_gc(&provider, &workspace, 0).await.unwrap();
    let objects = provider.objects.lock().unwrap();
    assert_eq!(
        objects
            .keys()
            .filter(|path| path.starts_with("v4/roots/"))
            .count(),
        1
    );
    assert_eq!(
        objects
            .keys()
            .filter(|path| path.starts_with("v4/shards/"))
            .count(),
        1
    );
    assert_eq!(
        objects
            .keys()
            .filter(|path| path.starts_with("v4/objects/"))
            .count(),
        1
    );
    assert!(!objects.keys().any(|path| path.starts_with("changes/")));
}

#[tokio::test]
async fn v4_head_acknowledgement_loss_is_idempotent() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    let cfg_a = config("device-a");
    let cfg_b = config("device-b");
    let workspace = v4::ensure_workspace(&provider, &cfg_a.device_id)
        .await
        .unwrap();
    let note = db_a.create_note("<p>durable v4</p>").unwrap();

    provider.fail_put_path_once("v4/heads/device-a.json");
    assert!(v4::push(&provider, &db_a, dir_a.path(), &cfg_a, &workspace)
        .await
        .is_err());
    assert!(!db_a.list_pending_changes(1).unwrap().is_empty());
    assert_eq!(
        v4::push(&provider, &db_a, dir_a.path(), &cfg_a, &workspace)
            .await
            .unwrap(),
        1
    );
    assert!(db_a.list_pending_changes(1).unwrap().is_empty());

    assert_eq!(
        v4::pull(&provider, &db_b, dir_b.path(), &cfg_b, &workspace)
            .await
            .unwrap(),
        (1, 0)
    );
    assert_eq!(
        db_b.get_note(&note.id).unwrap().unwrap().content,
        "<p>durable v4</p>"
    );
}

#[tokio::test]
async fn v4_new_device_restores_from_current_roots_without_history() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    let cfg_a = config("device-a");
    let cfg_b = config("device-b");
    let workspace = v4::ensure_workspace(&provider, &cfg_a.device_id)
        .await
        .unwrap();
    let note = db_a.create_note("<p>initial</p>").unwrap();
    v4::push(&provider, &db_a, dir_a.path(), &cfg_a, &workspace)
        .await
        .unwrap();
    for index in 0..10 {
        db_a.update_note(&note.id, &format!("<p>current {index}</p>"))
            .unwrap();
        v4::push(&provider, &db_a, dir_a.path(), &cfg_a, &workspace)
            .await
            .unwrap();
    }
    v4::run_gc(&provider, &workspace, 0).await.unwrap();
    v4::run_gc(&provider, &workspace, 0).await.unwrap();

    v4::pull(&provider, &db_b, dir_b.path(), &cfg_b, &workspace)
        .await
        .unwrap();
    assert_eq!(
        db_b.get_note(&note.id).unwrap().unwrap().content,
        "<p>current 9</p>"
    );
}

#[tokio::test]
async fn v4_preserves_binary_yjs_state_as_base64() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    let cfg_a = config("device-a");
    let cfg_b = config("device-b");
    let workspace = v4::ensure_workspace(&provider, &cfg_a.device_id)
        .await
        .unwrap();
    let note = db_a.create_note("<p>binary</p>").unwrap();
    let yjs = vec![0, 1, 2, 127, 128, 254, 255];
    db_a.update_note_with_yjs(&note.id, "<p>binary</p>", Some(&yjs))
        .unwrap();

    v4::push(&provider, &db_a, dir_a.path(), &cfg_a, &workspace)
        .await
        .unwrap();
    v4::pull(&provider, &db_b, dir_b.path(), &cfg_b, &workspace)
        .await
        .unwrap();
    assert_eq!(
        db_b.get_note(&note.id).unwrap().unwrap().yjs_state,
        Some(yjs)
    );
}

#[tokio::test]
async fn v4_concurrent_devices_converge_and_publish_conflict_copies() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    let cfg_a = config("device-a");
    let cfg_b = config("device-b");
    let workspace = v4::ensure_workspace(&provider, &cfg_a.device_id)
        .await
        .unwrap();
    let note = db_a.create_note("<p>seed</p>").unwrap();
    v4::push(&provider, &db_a, dir_a.path(), &cfg_a, &workspace)
        .await
        .unwrap();
    v4::pull(&provider, &db_b, dir_b.path(), &cfg_b, &workspace)
        .await
        .unwrap();

    db_a.update_note(&note.id, "<p>edit A</p>").unwrap();
    db_b.update_note(&note.id, "<p>edit B</p>").unwrap();
    v4::push(&provider, &db_a, dir_a.path(), &cfg_a, &workspace)
        .await
        .unwrap();
    v4::push(&provider, &db_b, dir_b.path(), &cfg_b, &workspace)
        .await
        .unwrap();

    for _ in 0..3 {
        v4::pull(&provider, &db_a, dir_a.path(), &cfg_a, &workspace)
            .await
            .unwrap();
        v4::pull(&provider, &db_b, dir_b.path(), &cfg_b, &workspace)
            .await
            .unwrap();
        v4::push(&provider, &db_a, dir_a.path(), &cfg_a, &workspace)
            .await
            .unwrap();
        v4::push(&provider, &db_b, dir_b.path(), &cfg_b, &workspace)
            .await
            .unwrap();
    }

    assert_eq!(
        db_a.get_note(&note.id).unwrap().unwrap().content,
        db_b.get_note(&note.id).unwrap().unwrap().content
    );
    let mut contents_a: Vec<_> = db_a
        .list_notes()
        .unwrap()
        .into_iter()
        .map(|item| item.title)
        .collect();
    let mut contents_b: Vec<_> = db_b
        .list_notes()
        .unwrap()
        .into_iter()
        .map(|item| item.title)
        .collect();
    contents_a.sort();
    contents_b.sort();
    assert_eq!(contents_a, contents_b);
}

#[tokio::test]
#[ignore = "destructive live WebDAV test; set QUICKNOTE_WEBDAV_TEST_URL"]
async fn live_v4_end_to_end_and_gc() {
    let endpoint =
        std::env::var("QUICKNOTE_WEBDAV_TEST_URL").expect("QUICKNOTE_WEBDAV_TEST_URL is required");
    let username = std::env::var("QUICKNOTE_WEBDAV_TEST_USERNAME").unwrap_or_default();
    let password = std::env::var("QUICKNOTE_WEBDAV_TEST_PASSWORD").unwrap_or_default();
    let provider = WebDavProvider::new(&endpoint, &username, &password).unwrap();
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let mut cfg_a = config(&format!("live-a-{}", Uuid::new_v4()));
    let mut cfg_b = config(&format!("live-b-{}", Uuid::new_v4()));
    cfg_a.endpoint = endpoint.clone();
    cfg_b.endpoint = endpoint;
    let workspace = v4::ensure_workspace(&provider, &cfg_a.device_id)
        .await
        .unwrap();
    let note = db_a.create_note("<p>live 0</p>").unwrap();
    let attachment_bytes: Vec<u8> = (0..4 * 1024 * 1024 + 17)
        .map(|index| ((index / (1024 * 1024)) + 0x70) as u8)
        .collect();
    let attachment_id = format!("{:x}", Sha256::digest(&attachment_bytes));
    let attachment_filename = format!("{attachment_id}.bin");
    std::fs::write(dir_a.path().join(&attachment_filename), &attachment_bytes).unwrap();
    db_a.register_attachment(
        &attachment_id,
        &attachment_filename,
        "application/octet-stream",
        attachment_bytes.len() as i64,
    )
    .unwrap();

    for index in 0..25 {
        db_a.update_note(&note.id, &format!("<p>live {index}</p>"))
            .unwrap();
        v4::push(&provider, &db_a, dir_a.path(), &cfg_a, &workspace)
            .await
            .unwrap();
    }
    v4::pull(&provider, &db_b, dir_b.path(), &cfg_b, &workspace)
        .await
        .unwrap();
    assert_eq!(
        db_b.get_note(&note.id).unwrap().unwrap().content,
        "<p>live 24</p>"
    );
    assert_eq!(
        std::fs::read(dir_b.path().join(&attachment_filename)).unwrap(),
        attachment_bytes
    );

    v4::run_gc(&provider, &workspace, 0).await.unwrap();
    v4::run_gc(&provider, &workspace, 0).await.unwrap();
    let status = v4::storage_status(&provider, &workspace).await.unwrap();
    assert_eq!(status.stored_objects, status.reachable_objects);
    assert_eq!(status.pending_gc_objects, 0);
    provider.delete("v4").await.unwrap();
}
