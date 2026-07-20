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
async fn multiple_edits_produce_single_state_file() {
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

    // Only one state file for this entity
    let state_files: Vec<String> = provider
        .objects
        .lock()
        .unwrap()
        .keys()
        .filter(|k| k.starts_with("state/") && !k.contains("/meta/"))
        .cloned()
        .collect();
    assert_eq!(state_files.len(), 1);
    assert!(state_files[0].contains(&note.id));
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
