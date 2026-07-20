use super::*;

#[test]
fn envelope_validates_device_and_schema() {
    let envelope = SyncEnvelope {
        schema_version: 1,
        device_id: "device-a".to_string(),
        seq: 7,
        entity_type: "note".to_string(),
        entity_id: "note-a".to_string(),
        operation: "upsert".to_string(),
        changed_at: "2026-01-01T00:00:00Z".to_string(),
        causal_version: None,
        yjs_update: None,
        note: Some(Note {
            id: "note-a".to_string(),
            title: "Note".to_string(),
            content: String::new(),
            yjs_state: None,
            yjs_state_version: 0,
            is_pinned: false,
            sort_order: 0,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            version: 1,
            is_deleted: false,
            tags: Vec::new(),
        }),
        attachment: None,
        clipboard: None,
        tag: None,
        note_tag: None,
    };
    assert!(validate_envelope(&envelope, "device-a").is_ok());
    assert!(validate_envelope(&envelope, "device-b").is_err());
    let mut version_two = envelope.clone();
    version_two.schema_version = 2;
    assert!(validate_envelope(&version_two, "device-a").is_err());
    version_two.causal_version = Some(CausalVersion::legacy("device-a", 7));
    assert!(validate_envelope(&version_two, "device-a").is_ok());
}

#[test]
fn attachment_rejects_traversal_and_tampered_content() {
    let bytes = b"image bytes";
    let id = format!("{:x}", Sha256::digest(bytes));
    let mut record = AttachmentRecord {
        id: id.clone(),
        relative_path: format!("{id}.webp"),
        mime_type: "image/webp".to_string(),
        size: bytes.len() as i64,
        created_at: "2026-01-01T00:00:00Z".to_string(),
    };
    assert!(validate_attachment(&record, bytes).is_ok());
    record.relative_path = format!("../{id}.webp");
    assert!(validate_attachment(&record, bytes).is_err());
    record.relative_path = format!("{id}.webp");
    assert!(validate_attachment(&record, b"tampered").is_err());
}

#[test]
fn safe_path_segment_validation() {
    assert!(is_safe_path_segment("device-a_1.test"));
    assert!(!is_safe_path_segment("../device-a"));
    assert!(!is_safe_path_segment(""));
    assert!(!is_safe_path_segment("has space"));
}

#[tokio::test]
async fn push_state_retries_after_failure() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().to_path_buf()).unwrap();
    db.create_note("<p>durable</p>").unwrap();
    let provider = MemoryProvider::default();
    provider.fail_after_next_put();
    let sync_config = config("device-a");

    assert!(push_state(&provider, &db, dir.path(), &sync_config)
        .await
        .is_err());
    assert_eq!(db.list_pending_changes(10).unwrap().len(), 1);

    assert_eq!(
        push_state(&provider, &db, dir.path(), &sync_config)
            .await
            .unwrap(),
        1
    );
    assert!(db.list_pending_changes(10).unwrap().is_empty());
}

#[tokio::test]
async fn invalid_remote_state_file_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    provider.objects.lock().unwrap().insert(
        "state/device-b/note/bad-note.json".to_string(),
        b"{not-json".to_vec(),
    );
    let sync_config = config("device-a");

    // pull_state skips invalid files gracefully
    let result = pull_state(&provider, &db, dir.path(), &sync_config).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), (0, 0));
}

#[tokio::test]
async fn missing_remote_attachment_fails_pull() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    let missing_bytes = b"missing";
    let id = format!("{:x}", Sha256::digest(missing_bytes));
    let envelope = SyncEnvelope {
        schema_version: 2,
        device_id: "device-b".to_string(),
        seq: 0,
        entity_type: "attachment".to_string(),
        entity_id: id.clone(),
        operation: "upsert".to_string(),
        changed_at: "2026-01-01T00:00:00Z".to_string(),
        causal_version: Some(CausalVersion::legacy("device-b", 1)),
        yjs_update: None,
        note: None,
        attachment: Some(AttachmentRecord {
            id: id.clone(),
            relative_path: format!("{id}.webp"),
            mime_type: "image/webp".to_string(),
            size: missing_bytes.len() as i64,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }),
        clipboard: None,
        tag: None,
        note_tag: None,
    };
    provider.objects.lock().unwrap().insert(
        format!("state/device-b/attachment/{id}.json"),
        serde_json::to_vec(&envelope).unwrap(),
    );
    let sync_config = config("device-a");

    // apply_envelope fails when the remote attachment bytes are not available
    assert!(pull_state(&provider, &db, dir.path(), &sync_config)
        .await
        .is_err());
}

#[tokio::test]
async fn pull_recovers_missing_local_attachment_file() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db_a = Database::new(dir_a.path().to_path_buf()).unwrap();
    let db_b = Database::new(dir_b.path().to_path_buf()).unwrap();
    let provider = MemoryProvider::default();
    let bytes = b"image bytes";
    let id = format!("{:x}", Sha256::digest(bytes));
    let filename = format!("{id}.webp");
    std::fs::write(dir_a.path().join(&filename), bytes).unwrap();
    db_a.register_attachment(&id, &filename, "image/webp", bytes.len() as i64)
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
    std::fs::remove_file(dir_b.path().join(&filename)).unwrap();

    assert_eq!(
        pull_state(&provider, &db_b, dir_b.path(), &config("device-b"))
            .await
            .unwrap(),
        (1, 0)
    );
    assert_eq!(std::fs::read(dir_b.path().join(&filename)).unwrap(), bytes);
}

#[test]
fn encrypt_decrypt_round_trip() {
    let mut cfg = SyncConfig::default();
    let salt = ensure_salt(&mut cfg);
    let key = derive_encryption_key(&cfg.device_id, &salt);
    let encrypted = encrypt_value("my-secret-password", &key).unwrap();
    let decrypted = decrypt_value(&encrypted, &key).unwrap();
    assert_eq!(decrypted, "my-secret-password");
}

#[test]
fn different_nonces_produce_different_ciphertexts() {
    let mut cfg = SyncConfig::default();
    let salt = ensure_salt(&mut cfg);
    let key = derive_encryption_key(&cfg.device_id, &salt);
    let e1 = encrypt_value("same-password", &key).unwrap();
    let e2 = encrypt_value("same-password", &key).unwrap();
    assert_ne!(e1, e2);
}

#[test]
fn wrong_key_fails_decryption() {
    let mut cfg = SyncConfig::default();
    let salt = ensure_salt(&mut cfg);
    let key = derive_encryption_key(&cfg.device_id, &salt);
    let encrypted = encrypt_value("secret", &key).unwrap();
    let wrong_key = derive_encryption_key("wrong-device-id", &salt);
    assert!(decrypt_value(&encrypted, &wrong_key).is_err());
}

#[test]
fn store_and_retrieve_webdav_password() {
    let mut cfg = SyncConfig::default();
    store_webdav_password(&mut cfg, "test-password").unwrap();
    assert_eq!(get_webdav_password(&cfg).unwrap(), "test-password");
}

#[test]
fn password_survives_json_round_trip() {
    let mut cfg = SyncConfig::default();
    store_webdav_password(&mut cfg, "persistent-pw").unwrap();
    let json = serde_json::to_string(&cfg).unwrap();
    let restored: SyncConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(get_webdav_password(&restored).unwrap(), "persistent-pw");
}
