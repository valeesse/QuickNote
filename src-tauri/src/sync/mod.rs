mod webdav;

use crate::db::{AttachmentRecord, CausalVersion, ClipboardItem, Database, Note, SyncChange};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Component;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;
use uuid::Uuid;
use webdav::WebDavProvider;

const PROVIDER_NAME: &str = "webdav";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub enabled: bool,
    pub provider: String,
    pub endpoint: String,
    pub username: String,
    pub device_id: String,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: PROVIDER_NAME.to_string(),
            endpoint: String::new(),
            username: String::new(),
            device_id: Uuid::new_v4().to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SyncConfigInput {
    pub enabled: bool,
    pub provider: String,
    pub endpoint: String,
    pub username: String,
    pub password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SyncReport {
    pub pushed: usize,
    pub pulled: usize,
    pub conflicts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SyncEnvelope {
    schema_version: u32,
    device_id: String,
    seq: i64,
    entity_type: String,
    entity_id: String,
    operation: String,
    changed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    causal_version: Option<CausalVersion>,
    note: Option<Note>,
    attachment: Option<AttachmentRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clipboard: Option<ClipboardItem>,
}

#[async_trait]
pub trait SyncProvider: Send + Sync {
    async fn prepare(&self, device_id: &str) -> Result<(), String>;
    async fn list(&self, path: &str) -> Result<Vec<String>, String>;
    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, String>;
    async fn put(&self, path: &str, body: Vec<u8>, content_type: &str) -> Result<(), String>;
}

pub struct SyncService {
    config_path: PathBuf,
    sync_lock: Mutex<()>,
}

impl SyncService {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            sync_lock: Mutex::new(()),
        }
    }

    pub fn get_config(&self) -> Result<SyncConfig, String> {
        if !self.config_path.exists() {
            let config = SyncConfig::default();
            let data = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
            std::fs::write(&self.config_path, data)
                .map_err(|e| format!("Failed to initialize sync config: {e}"))?;
            return Ok(config);
        }
        let data = std::fs::read(&self.config_path)
            .map_err(|e| format!("Failed to read sync config: {e}"))?;
        serde_json::from_slice(&data).map_err(|e| format!("Invalid sync config: {e}"))
    }

    pub fn set_config(&self, input: SyncConfigInput) -> Result<SyncConfig, String> {
        if input.provider != PROVIDER_NAME {
            return Err(format!("Unsupported sync provider: {}", input.provider));
        }
        if input.enabled
            && (!input.endpoint.starts_with("https://") || input.username.trim().is_empty())
        {
            return Err("WebDAV sync requires an HTTPS endpoint and username".to_string());
        }

        let existing = self.get_config().unwrap_or_default();
        let config = SyncConfig {
            enabled: input.enabled,
            provider: input.provider,
            endpoint: input.endpoint.trim_end_matches('/').to_string(),
            username: input.username.trim().to_string(),
            device_id: existing.device_id,
        };
        if let Some(password) = input.password.filter(|value| !value.is_empty()) {
            keyring_entry(&config)?
                .set_password(&password)
                .map_err(|e| e.to_string())?;
        }
        let data = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
        std::fs::write(&self.config_path, data)
            .map_err(|e| format!("Failed to write sync config: {e}"))?;
        Ok(config)
    }

    pub async fn sync(&self, db: &Database, attachments_dir: &Path) -> Result<SyncReport, String> {
        let _guard = self.sync_lock.lock().await;
        let config = self.get_config()?;
        if !config.enabled {
            return Err("Sync is not enabled".to_string());
        }
        let password = keyring_entry(&config)?
            .get_password()
            .map_err(|_| "WebDAV password is missing; save sync settings again".to_string())?;
        let provider = WebDavProvider::new(&config.endpoint, &config.username, &password)?;
        provider.prepare(&config.device_id).await?;
        db.ensure_sync_bootstrap(&format!("{}:{}", config.provider, config.endpoint))
            .map_err(|e| e.to_string())?;

        let (pulled, conflicts) = pull_changes(&provider, db, attachments_dir, &config).await?;
        let pushed = push_changes(&provider, db, attachments_dir, &config).await?;
        Ok(SyncReport {
            pushed,
            pulled,
            conflicts,
        })
    }
}

fn keyring_entry(config: &SyncConfig) -> Result<keyring::Entry, String> {
    keyring::Entry::new("com.quicknote.desktop.sync", &config.device_id).map_err(|e| e.to_string())
}

async fn push_changes(
    provider: &dyn SyncProvider,
    db: &Database,
    attachments_dir: &Path,
    config: &SyncConfig,
) -> Result<usize, String> {
    let changes = db.list_pending_changes(500).map_err(|e| e.to_string())?;
    let mut pushed = 0;
    for change in changes {
        let envelope = build_envelope(db, &change, &config.device_id)?;
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
        provider
            .put(
                &format!("changes/{}/{:020}.json", config.device_id, change.seq),
                body,
                "application/json",
            )
            .await?;
        db.mark_change_synced(change.seq)
            .map_err(|e| e.to_string())?;
        pushed += 1;
    }
    Ok(pushed)
}

fn build_envelope(
    db: &Database,
    change: &SyncChange,
    device_id: &str,
) -> Result<SyncEnvelope, String> {
    let mut operation = change.operation.clone();
    let mut note = None;
    let mut attachment = None;
    let mut clipboard = None;
    let causal_version = db
        .ensure_local_causal_version(&change.entity_type, &change.entity_id, device_id)
        .map_err(|e| e.to_string())?;
    if change.entity_type == "note" {
        note = db
            .get_note_for_sync(&change.entity_id)
            .map_err(|e| e.to_string())?;
        if note.as_ref().is_some_and(|value| value.is_deleted) || note.is_none() {
            operation = "delete".to_string();
        }
    } else if change.entity_type == "attachment" {
        attachment = db
            .get_attachment(&change.entity_id)
            .map_err(|e| e.to_string())?;
        if attachment.is_none() {
            operation = "delete".to_string();
        }
    } else if change.entity_type == "clipboard" {
        clipboard = db
            .get_clipboard_item_for_sync(&change.entity_id)
            .map_err(|e| e.to_string())?;
        if clipboard.as_ref().is_some_and(|item| item.is_deleted) || clipboard.is_none() {
            operation = "delete".to_string();
        }
    }
    Ok(SyncEnvelope {
        schema_version: 2,
        device_id: device_id.to_string(),
        seq: change.seq,
        entity_type: change.entity_type.clone(),
        entity_id: change.entity_id.clone(),
        operation,
        changed_at: change.changed_at.clone(),
        causal_version: Some(causal_version),
        note,
        attachment,
        clipboard,
    })
}

async fn pull_changes(
    provider: &dyn SyncProvider,
    db: &Database,
    attachments_dir: &Path,
    config: &SyncConfig,
) -> Result<(usize, usize), String> {
    let mut pulled = 0;
    let mut conflicts = 0;
    let cursor_scope = format!("{}:{}", config.provider, config.endpoint);
    for device_id in provider.list("changes").await? {
        if device_id == config.device_id || !is_safe_path_segment(&device_id) {
            continue;
        }
        let mut cursor = db
            .get_sync_cursor(&cursor_scope, &device_id)
            .map_err(|e| e.to_string())?;
        let mut files = provider.list(&format!("changes/{device_id}")).await?;
        files.sort();
        for file in files {
            let Some(seq) = parse_change_sequence(&file) else {
                continue;
            };
            if seq <= cursor {
                continue;
            }
            let path = format!("changes/{device_id}/{file}");
            let Some(body) = provider.get(&path).await? else {
                continue;
            };
            let envelope: SyncEnvelope = serde_json::from_slice(&body)
                .map_err(|e| format!("Invalid remote change {path}: {e}"))?;
            validate_envelope(&envelope, &device_id, seq)
                .map_err(|e| format!("Invalid remote change {path}: {e}"))?;
            let (changed, conflict) =
                apply_envelope(provider, db, attachments_dir, &envelope, &config.device_id).await?;
            if changed {
                pulled += 1;
            }
            if conflict {
                conflicts += 1;
            }
            cursor = seq;
            db.set_sync_cursor(&cursor_scope, &device_id, cursor)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok((pulled, conflicts))
}

fn is_safe_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn parse_change_sequence(filename: &str) -> Option<i64> {
    let digits = filename.strip_suffix(".json")?;
    if digits.len() != 20 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

async fn apply_envelope(
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
                let Some(bytes) = provider.get(&format!("attachments/{}", record.id)).await? else {
                    return Err(format!("Remote attachment {} is missing", record.id));
                };
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
        "clipboard" => {
            let Some(item) = &envelope.clipboard else {
                return Ok((false, false));
            };
            db.apply_remote_clipboard(item, &causal_version, local_device_id)
                .map(|changed| (changed, false))
                .map_err(|e| e.to_string())
        }
        _ => Ok((false, false)),
    }
}

fn validate_envelope(
    envelope: &SyncEnvelope,
    expected_device: &str,
    expected_seq: i64,
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
    if envelope.device_id != expected_device || envelope.seq != expected_seq {
        return Err("device or sequence does not match its immutable path".to_string());
    }
    if !matches!(
        envelope.entity_type.as_str(),
        "note" | "attachment" | "clipboard"
    ) {
        return Err(format!("unsupported entity type {}", envelope.entity_type));
    }
    if !matches!(envelope.operation.as_str(), "upsert" | "delete") {
        return Err(format!("unsupported operation {}", envelope.operation));
    }
    if envelope.entity_type == "note" && envelope.operation == "upsert"
        && envelope.note.as_ref().map(|note| note.id.as_str()) != Some(envelope.entity_id.as_str())
        {
            return Err("note payload does not match its entity ID".to_string());
        }
    if envelope.entity_type == "attachment" && envelope.operation == "upsert"
        && envelope.attachment.as_ref().map(|item| item.id.as_str())
            != Some(envelope.entity_id.as_str())
        {
            return Err("attachment payload does not match its entity ID".to_string());
        }
    if envelope.entity_type == "clipboard"
        && envelope.clipboard.as_ref().map(|item| item.id.as_str())
            != Some(envelope.entity_id.as_str())
    {
        return Err("clipboard payload does not match its entity ID".to_string());
    }
    Ok(())
}

fn validate_attachment(record: &AttachmentRecord, bytes: &[u8]) -> Result<(), String> {
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

fn validate_attachment_path(record: &AttachmentRecord) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct MemoryProvider {
        objects: StdMutex<BTreeMap<String, Vec<u8>>>,
        fail_after_next_put: AtomicBool,
    }

    impl MemoryProvider {
        fn fail_after_next_put(&self) {
            self.fail_after_next_put.store(true, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl SyncProvider for MemoryProvider {
        async fn prepare(&self, _device_id: &str) -> Result<(), String> {
            Ok(())
        }

        async fn list(&self, path: &str) -> Result<Vec<String>, String> {
            let prefix = format!("{}/", path.trim_matches('/'));
            let objects = self.objects.lock().unwrap();
            let mut children = BTreeSet::new();
            for key in objects.keys().filter(|key| key.starts_with(&prefix)) {
                if let Some(child) = key[prefix.len()..].split('/').next() {
                    if !child.is_empty() {
                        children.insert(child.to_string());
                    }
                }
            }
            Ok(children.into_iter().collect())
        }

        async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self.objects.lock().unwrap().get(path).cloned())
        }

        async fn put(&self, path: &str, body: Vec<u8>, _content_type: &str) -> Result<(), String> {
            let mut objects = self.objects.lock().unwrap();
            if let Some(existing) = objects.get(path) {
                if existing != &body {
                    return Err(format!("immutable collision at {path}"));
                }
            } else {
                objects.insert(path.to_string(), body);
            }
            if self.fail_after_next_put.swap(false, Ordering::SeqCst) {
                return Err("injected acknowledgement loss".to_string());
            }
            Ok(())
        }
    }

    fn config(device_id: &str) -> SyncConfig {
        SyncConfig {
            enabled: true,
            provider: "webdav".to_string(),
            endpoint: "https://dav.test/quicknote".to_string(),
            username: "tester".to_string(),
            device_id: device_id.to_string(),
        }
    }

    #[test]
    fn envelope_identity_must_match_immutable_path() {
        let envelope = SyncEnvelope {
            schema_version: 1,
            device_id: "device-a".to_string(),
            seq: 7,
            entity_type: "note".to_string(),
            entity_id: "note-a".to_string(),
            operation: "upsert".to_string(),
            changed_at: "2026-01-01T00:00:00Z".to_string(),
            causal_version: None,
            note: Some(Note {
                id: "note-a".to_string(),
                title: "Note".to_string(),
                content: String::new(),
                is_pinned: false,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                version: 1,
                is_deleted: false,
            }),
            attachment: None,
            clipboard: None,
        };
        assert!(validate_envelope(&envelope, "device-a", 7).is_ok());
        assert!(validate_envelope(&envelope, "device-b", 7).is_err());
        assert!(validate_envelope(&envelope, "device-a", 8).is_err());
        let mut version_two = envelope.clone();
        version_two.schema_version = 2;
        assert!(validate_envelope(&version_two, "device-a", 7).is_err());
        version_two.causal_version = Some(CausalVersion::legacy("device-a", 7));
        assert!(validate_envelope(&version_two, "device-a", 7).is_ok());
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
    fn only_canonical_change_filenames_advance_a_cursor() {
        assert_eq!(parse_change_sequence("00000000000000000042.json"), Some(42));
        assert_eq!(parse_change_sequence("42.json"), None);
        assert_eq!(parse_change_sequence("00000000000000000042.json.bak"), None);
        assert!(is_safe_path_segment("device-a_1.test"));
        assert!(!is_safe_path_segment("../device-a"));
    }

    #[tokio::test]
    async fn acknowledgement_loss_retries_same_immutable_change_before_clearing_outbox() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        db.create_note("<p>durable</p>").unwrap();
        let provider = MemoryProvider::default();
        provider.fail_after_next_put();
        let sync_config = config("device-a");

        assert!(push_changes(&provider, &db, dir.path(), &sync_config)
            .await
            .is_err());
        assert_eq!(db.list_pending_changes(10).unwrap().len(), 1);
        assert_eq!(provider.objects.lock().unwrap().len(), 1);

        assert_eq!(
            push_changes(&provider, &db, dir.path(), &sync_config)
                .await
                .unwrap(),
            1
        );
        assert!(db.list_pending_changes(10).unwrap().is_empty());
        assert_eq!(provider.objects.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn invalid_remote_change_does_not_advance_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let provider = MemoryProvider::default();
        provider.objects.lock().unwrap().insert(
            "changes/device-b/00000000000000000001.json".to_string(),
            b"{not-json".to_vec(),
        );
        let sync_config = config("device-a");

        assert!(pull_changes(&provider, &db, dir.path(), &sync_config)
            .await
            .is_err());
        let scope = format!("{}:{}", sync_config.provider, sync_config.endpoint);
        assert_eq!(db.get_sync_cursor(&scope, "device-b").unwrap(), 0);
    }

    #[tokio::test]
    async fn missing_remote_attachment_does_not_advance_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let provider = MemoryProvider::default();
        let missing_bytes = b"missing";
        let id = format!("{:x}", Sha256::digest(missing_bytes));
        let envelope = SyncEnvelope {
            schema_version: 2,
            device_id: "device-b".to_string(),
            seq: 1,
            entity_type: "attachment".to_string(),
            entity_id: id.clone(),
            operation: "upsert".to_string(),
            changed_at: "2026-01-01T00:00:00Z".to_string(),
            causal_version: Some(CausalVersion::legacy("device-b", 1)),
            note: None,
            attachment: Some(AttachmentRecord {
                id: id.clone(),
                relative_path: format!("{id}.webp"),
                mime_type: "image/webp".to_string(),
                size: missing_bytes.len() as i64,
                created_at: "2026-01-01T00:00:00Z".to_string(),
            }),
            clipboard: None,
        };
        provider.objects.lock().unwrap().insert(
            "changes/device-b/00000000000000000001.json".to_string(),
            serde_json::to_vec(&envelope).unwrap(),
        );
        let sync_config = config("device-a");

        assert!(pull_changes(&provider, &db, dir.path(), &sync_config)
            .await
            .is_err());
        let scope = format!("{}:{}", sync_config.provider, sync_config.endpoint);
        assert_eq!(db.get_sync_cursor(&scope, "device-b").unwrap(), 0);
    }

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
            push_changes(&provider, &db_a, dir_a.path(), &config("device-a"))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            pull_changes(&provider, &db_b, dir_b.path(), &config("device-b"))
                .await
                .unwrap(),
            (1, 0)
        );
        assert_eq!(
            db_b.get_clipboard_item(&item.id).unwrap().unwrap().content,
            item.content
        );
    }
}
