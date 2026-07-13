mod cloud;
mod webdav;

use crate::db::{AttachmentRecord, Database, SyncChange};
use async_trait::async_trait;
pub use quicknote_protocol::SyncEnvelope;
use quicknote_protocol::{CausalRelation, CausalVersion};
use serde::{Deserialize, Serialize};
use aes_gcm::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
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
    #[serde(default)]
    pub cloud_enabled: bool,
    #[serde(default)]
    pub cloud_url: String,
    #[serde(default)]
    pub cloud_email: String,
    #[serde(default)]
    pub cloud_cursor_seq: i64,
    #[serde(default)]
    pub cloud_token_created_at: i64,
    #[serde(default)]
    pub password_salt: Option<String>,
    #[serde(default)]
    pub webdav_password_encrypted: Option<String>,
    #[serde(default)]
    pub cloud_password_encrypted: Option<String>,
    #[serde(default)]
    pub cloud_token_encrypted: Option<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: PROVIDER_NAME.to_string(),
            endpoint: String::new(),
            username: String::new(),
            device_id: Uuid::new_v4().to_string(),
            cloud_enabled: false,
            cloud_url: String::new(),
            cloud_email: String::new(),
            cloud_cursor_seq: 0,
            cloud_token_created_at: 0,
            password_salt: None,
            webdav_password_encrypted: None,
            cloud_password_encrypted: None,
            cloud_token_encrypted: None,
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
    #[serde(default)]
    pub cloud_enabled: bool,
    #[serde(default)]
    pub cloud_url: String,
    #[serde(default)]
    pub cloud_email: String,
    pub cloud_password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SyncReport {
    pub pushed: usize,
    pub pulled: usize,
    pub conflicts: usize,
}

#[async_trait]
pub trait SyncProvider: Send + Sync {
    async fn prepare(&self, device_id: &str) -> Result<(), String>;
    async fn list(&self, path: &str) -> Result<Vec<String>, String>;
    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, String>;
    async fn put(&self, path: &str, body: Vec<u8>, content_type: &str) -> Result<(), String>;
    #[allow(dead_code)]
    async fn delete(&self, path: &str) -> Result<(), String>;
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
        if input.provider != PROVIDER_NAME && !input.provider.is_empty() {
            return Err(format!("Unsupported sync provider: {}", input.provider));
        }
        let endpoint = input.endpoint.trim().trim_end_matches('/').to_string();
        let username = input.username.trim().to_string();
        let cloud_url = input.cloud_url.trim().trim_end_matches('/').to_string();
        let cloud_email = input.cloud_email.trim().to_string();
        if input.enabled
            && (!endpoint.starts_with("https://") && !endpoint.starts_with("http://")
                || username.is_empty())
        {
            return Err("WebDAV sync requires an HTTP(S) endpoint and username".to_string());
        }
        if input.cloud_enabled && input.enabled {
            return Err("Cloud mode and direct WebDAV mode are mutually exclusive".to_string());
        }
        if input.cloud_enabled
            && (!cloud_url.starts_with("https://") && !cloud_url.starts_with("http://")
                || cloud_email.is_empty())
        {
            return Err("Cloud sync requires an HTTP(S) URL and email".to_string());
        }

        let existing = self.get_config().unwrap_or_default();
        let webdav_identity_changed =
            existing.endpoint != endpoint || existing.username != username;
        let cloud_identity_changed =
            existing.cloud_url != cloud_url || existing.cloud_email != cloud_email;
        let mut config = SyncConfig {
            enabled: input.enabled,
            provider: input.provider,
            endpoint,
            username,
            device_id: existing.device_id.clone(),
            cloud_enabled: input.cloud_enabled,
            cloud_url,
            cloud_email,
            cloud_cursor_seq: if cloud_identity_changed {
                0
            } else {
                existing.cloud_cursor_seq
            },
            cloud_token_created_at: if cloud_identity_changed {
                0
            } else {
                existing.cloud_token_created_at
            },
            password_salt: existing.password_salt.clone(),
            webdav_password_encrypted: existing.webdav_password_encrypted.clone(),
            cloud_password_encrypted: existing.cloud_password_encrypted.clone(),
            cloud_token_encrypted: existing.cloud_token_encrypted.clone(),
        };
        if let Some(password) = input.password.filter(|value| !value.is_empty()) {
            store_webdav_password(&mut config, &password)?;
        } else if config.enabled {
            if webdav_identity_changed {
                if let Ok(old_password) = get_webdav_password(&existing) {
                    store_webdav_password(&mut config, &old_password)?;
                } else {
                    return Err(
                        "WebDAV endpoint or username changed; please re-enter the password"
                            .to_string(),
                    );
                }
            } else {
                get_webdav_password(&config)?;
            }
        }
        let has_new_cloud_password = input
            .cloud_password
            .as_deref()
            .is_some_and(|value| !value.is_empty());
        if let Some(cloud_pw) = input
            .cloud_password
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            if !config.cloud_url.is_empty() && !config.cloud_email.is_empty() {
                store_cloud_password(&mut config, cloud_pw)?;
                delete_cloud_token(&mut config);
            }
        } else if config.cloud_enabled {
            if cloud_identity_changed {
                if let Ok(old_password) = get_cloud_password(&existing) {
                    store_cloud_password(&mut config, &old_password)?;
                } else {
                    return Err(
                        "Cloud account changed; please re-enter the cloud password".to_string(),
                    );
                }
            } else {
                get_cloud_password(&config)?;
            }
        }
        if cloud_identity_changed && !has_new_cloud_password {
            delete_cloud_token(&mut config);
        }
        let data = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
        std::fs::write(&self.config_path, data)
            .map_err(|e| format!("Failed to write sync config: {e}"))?;
        Ok(config)
    }

    pub async fn sync(&self, db: &Database, attachments_dir: &Path) -> Result<SyncReport, String> {
        let _guard = self.sync_lock.lock().await;
        let mut config = self.get_config()?;

        let mut total_pushed = 0;
        let mut total_pulled = 0;
        let mut total_conflicts = 0;

        // Cloud sync path
        if config.cloud_enabled && !config.cloud_url.is_empty() {
            let cloud_token = self.get_cloud_token(&mut config).await?;
            let cloud = cloud::CloudProvider::new(&config.cloud_url, &cloud_token)?;
            db.ensure_sync_bootstrap(&cloud_bootstrap_scope(&config))
                .map_err(|e| e.to_string())?;

            // Pull from cloud
            let (envelopes, server_seq) = cloud.pull(config.cloud_cursor_seq).await?;
            for envelope in &envelopes {
                let (changed, conflict) =
                    apply_envelope(&cloud, db, attachments_dir, envelope, &config.device_id)
                        .await?;
                if changed {
                    total_pulled += 1;
                }
                if conflict {
                    total_conflicts += 1;
                }
            }
            config.cloud_cursor_seq = server_seq;

            // Push local changes to cloud
            let changes = db.list_pending_changes(500).map_err(|e| e.to_string())?;
            let mut cloud_envelopes = Vec::new();
            for change in &changes {
                if let Ok(envelope) = build_envelope(db, change, &config.device_id) {
                    if let Some(attachment) = &envelope.attachment {
                        let bytes = std::fs::read(attachments_dir.join(&attachment.relative_path))
                            .map_err(|error| {
                                format!("Failed to read attachment {}: {error}", attachment.id)
                            })?;
                        cloud
                            .put(
                                &format!("attachments/{}", attachment.id),
                                bytes,
                                &attachment.mime_type,
                            )
                            .await?;
                        db.mark_change_synced(change.seq)
                            .map_err(|error| error.to_string())?;
                        total_pushed += 1;
                    } else {
                        cloud_envelopes.push(envelope);
                    }
                }
            }
            if !cloud_envelopes.is_empty() {
                let response = cloud.push(&cloud_envelopes).await?;
                total_pushed += response.accepted;
                total_conflicts += response.conflicts;
                for sequence in response.acknowledged_sequences {
                    db.mark_change_synced(sequence)
                        .map_err(|error| error.to_string())?;
                }
            }

            // Save updated cursor
            self.save_config(&config)?;
        } else if config.enabled {
            // WebDAV-only sync (existing logic)
            let (wp, (pulled, conflicts)) = self.webdav_sync(db, attachments_dir, &config).await?;
            total_pushed = wp;
            total_pulled = pulled;
            total_conflicts = conflicts;
        } else {
            return Err("Sync is not enabled".to_string());
        }

        Ok(SyncReport {
            pushed: total_pushed,
            pulled: total_pulled,
            conflicts: total_conflicts,
        })
    }

    async fn webdav_sync(
        &self,
        db: &Database,
        attachments_dir: &Path,
        config: &SyncConfig,
    ) -> Result<(usize, (usize, usize)), String> {
        let password = get_webdav_password(config)?;
        let provider = WebDavProvider::new(&config.endpoint, &config.username, &password)?;
        provider.prepare(&config.device_id).await?;
        db.ensure_sync_bootstrap(&format!("{}:{}", config.provider, config.endpoint))
            .map_err(|e| e.to_string())?;

        let pulled_conflicts = pull_state(&provider, db, attachments_dir, config).await?;
        let pushed = push_state(&provider, db, attachments_dir, config).await?;
        Ok((pushed, pulled_conflicts))
    }

    async fn get_cloud_token(&self, config: &mut SyncConfig) -> Result<String, String> {
        let now = chrono::Utc::now().timestamp();
        if now - config.cloud_token_created_at < 6 * 24 * 60 * 60 {
            if let Ok(token) = get_cloud_token_from_config(config) {
                return Ok(token);
            }
        }
        let password = get_cloud_password(config)?;
        let login =
            cloud::CloudProvider::login(&config.cloud_url, &config.cloud_email, &password).await?;
        store_cloud_token(config, &login.token)?;
        config.cloud_token_created_at = now;
        Ok(login.token)
    }

    fn save_config(&self, config: &SyncConfig) -> Result<(), String> {
        let data = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
        std::fs::write(&self.config_path, data)
            .map_err(|e| format!("Failed to save sync config: {e}"))?;
        Ok(())
    }

    /// Test the stored WebDAV connection using the saved encrypted password.
    pub async fn test_stored_webdav(&self) -> Result<(), String> {
        let config = self.get_config()?;
        if !config.enabled {
            return Err("WebDAV sync is not enabled".to_string());
        }
        let password = get_webdav_password(&config)?;
        test_webdav_connection(&config.endpoint, &config.username, &password).await
    }
}

/// Test a WebDAV connection by issuing a PROPFIND on the root endpoint.
pub async fn test_webdav_connection(
    endpoint: &str,
    username: &str,
    password: &str,
) -> Result<(), String> {
    let provider = webdav::WebDavProvider::new(endpoint, username, password)?;
    // PROPFIND on the root — a 207 or 200 means the server accepted our credentials.
    let _items = provider.list("").await?;
    Ok(())
}

/// Test a cloud connection by attempting a login.
pub async fn test_cloud_connection(
    cloud_url: &str,
    cloud_email: &str,
    cloud_password: &str,
) -> Result<(), String> {
    cloud::CloudProvider::login(cloud_url, cloud_email, cloud_password).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// AES-256-GCM encrypted credential storage (replaces keyring)
// ---------------------------------------------------------------------------

fn derive_encryption_key(device_id: &str, salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(device_id.as_bytes());
    hasher.update([0]);
    hasher.update(salt);
    hasher.finalize().into()
}

fn encrypt_value(plaintext: &str, key: &[u8; 32]) -> Result<String, String> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| format!("Encryption init failed: {e}"))?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {e}"))?;
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(&combined))
}

fn cloud_bootstrap_scope(config: &SyncConfig) -> String {
    format!("cloud:{}:{}", config.cloud_url, config.cloud_email)
}

fn decrypt_value(encoded: &str, key: &[u8; 32]) -> Result<String, String> {
    let combined = BASE64
        .decode(encoded)
        .map_err(|e| format!("Base64 decode failed: {e}"))?;
    if combined.len() < 13 {
        return Err("Encrypted value is too short".to_string());
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| format!("Decryption init failed: {e}"))?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed (key may have changed): {e}"))?;
    String::from_utf8(plaintext).map_err(|e| format!("Decrypted value is not valid UTF-8: {e}"))
}

fn ensure_salt(config: &mut SyncConfig) -> Vec<u8> {
    if let Some(ref salt_b64) = config.password_salt {
        if let Ok(salt) = BASE64.decode(salt_b64) {
            return salt;
        }
    }
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    config.password_salt = Some(BASE64.encode(salt));
    salt.to_vec()
}

fn encryption_key_for(config: &SyncConfig) -> Result<[u8; 32], String> {
    let salt_b64 = config
        .password_salt
        .as_ref()
        .ok_or_else(|| "Encryption salt is missing".to_string())?;
    let salt_bytes = BASE64
        .decode(salt_b64)
        .map_err(|e| format!("Invalid salt: {e}"))?;
    Ok(derive_encryption_key(&config.device_id, &salt_bytes))
}

fn store_webdav_password(config: &mut SyncConfig, password: &str) -> Result<(), String> {
    let salt = ensure_salt(config);
    let key = derive_encryption_key(&config.device_id, &salt);
    config.webdav_password_encrypted = Some(encrypt_value(password, &key)?);
    Ok(())
}

fn get_webdav_password(config: &SyncConfig) -> Result<String, String> {
    let encrypted = config
        .webdav_password_encrypted
        .as_ref()
        .ok_or_else(|| "WebDAV password is missing; save sync settings again".to_string())?;
    let key = encryption_key_for(config)?;
    decrypt_value(encrypted, &key)
}

fn store_cloud_password(config: &mut SyncConfig, password: &str) -> Result<(), String> {
    let salt = ensure_salt(config);
    let key = derive_encryption_key(&config.device_id, &salt);
    config.cloud_password_encrypted = Some(encrypt_value(password, &key)?);
    Ok(())
}

fn get_cloud_password(config: &SyncConfig) -> Result<String, String> {
    let encrypted = config
        .cloud_password_encrypted
        .as_ref()
        .ok_or_else(|| "Cloud password is missing; save cloud settings again".to_string())?;
    let key = encryption_key_for(config)?;
    decrypt_value(encrypted, &key)
}

fn store_cloud_token(config: &mut SyncConfig, token: &str) -> Result<(), String> {
    let salt = ensure_salt(config);
    let key = derive_encryption_key(&config.device_id, &salt);
    config.cloud_token_encrypted = Some(encrypt_value(token, &key)?);
    Ok(())
}

fn get_cloud_token_from_config(config: &SyncConfig) -> Result<String, String> {
    let encrypted = config
        .cloud_token_encrypted
        .as_ref()
        .ok_or_else(|| "Cloud token is missing".to_string())?;
    let key = encryption_key_for(config)?;
    decrypt_value(encrypted, &key)
}

fn delete_cloud_token(config: &mut SyncConfig) {
    config.cloud_token_encrypted = None;
}

fn state_file_path(device_id: &str, entity_type: &str, entity_id: &str) -> String {
    format!("state/{device_id}/{entity_type}/{entity_id}.json")
}

async fn push_state(
    provider: &dyn SyncProvider,
    db: &Database,
    attachments_dir: &Path,
    config: &SyncConfig,
) -> Result<usize, String> {
    let changes = db.list_pending_changes(500).map_err(|e| e.to_string())?;

    // Deduplicate: collect unique (entity_type, entity_id) pairs
    let mut seen = std::collections::HashSet::new();
    let mut entities: Vec<(String, String)> = Vec::new();
    for change in &changes {
        let key = (change.entity_type.clone(), change.entity_id.clone());
        if seen.insert(key.clone()) {
            entities.push(key);
        }
    }

    let mut pushed = 0;
    for (entity_type, entity_id) in &entities {
        let envelope =
            build_state_envelope(db, entity_type, entity_id, &config.device_id)?;

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
        let path = state_file_path(&config.device_id, entity_type, entity_id);
        provider.put(&path, body, "application/json").await?;
        db.mark_entity_changes_synced(entity_type, entity_id)
            .map_err(|e| e.to_string())?;
        pushed += 1;
    }
    Ok(pushed)
}

fn build_state_envelope(
    db: &Database,
    entity_type: &str,
    entity_id: &str,
    device_id: &str,
) -> Result<SyncEnvelope, String> {
    let causal_version = db
        .ensure_local_causal_version(entity_type, entity_id, device_id)
        .map_err(|e| e.to_string())?;

    let mut operation = "upsert".to_string();
    let mut note = None;
    let mut attachment = None;
    let mut clipboard = None;
    let mut tag = None;
    let mut note_tag = None;
    let mut changed_at = chrono::Utc::now().to_rfc3339();

    match entity_type {
        "note" => {
            note = db.get_note_for_sync(entity_id).map_err(|e| e.to_string())?;
            if let Some(ref n) = note {
                changed_at = n.updated_at.clone();
            }
            if note.as_ref().is_some_and(|n| n.is_deleted) || note.is_none() {
                operation = "delete".to_string();
                note = None;
            }
        }
        "attachment" => {
            attachment = db.get_attachment(entity_id).map_err(|e| e.to_string())?;
            if let Some(ref a) = attachment {
                changed_at = a.created_at.clone();
            }
            if attachment.is_none() {
                operation = "delete".to_string();
            }
        }
        "clipboard" => {
            clipboard = db
                .get_clipboard_item_for_sync(entity_id)
                .map_err(|e| e.to_string())?;
            if let Some(ref c) = clipboard {
                changed_at = c.updated_at.clone();
            }
            if clipboard.as_ref().is_some_and(|c| c.is_deleted) || clipboard.is_none() {
                operation = "delete".to_string();
                clipboard = None;
            }
        }
        "tag" => {
            tag = db.get_tag_for_sync(entity_id).map_err(|e| e.to_string())?;
            if let Some(ref value) = tag {
                changed_at = value.updated_at.clone();
            }
            if tag.as_ref().is_some_and(|value| value.is_deleted) || tag.is_none() {
                operation = "delete".to_string();
                tag = None;
            }
        }
        "note_tag" => {
            note_tag = db
                .get_note_tag_for_sync(entity_id)
                .map_err(|e| e.to_string())?;
            if let Some(ref value) = note_tag {
                changed_at = value.created_at.clone();
            }
            if note_tag.is_none() {
                operation = "delete".to_string();
            }
        }
        _ => {}
    }

    Ok(SyncEnvelope {
        schema_version: 2,
        device_id: device_id.to_string(),
        seq: 0,
        entity_type: entity_type.to_string(),
        entity_id: entity_id.to_string(),
        operation,
        changed_at,
        causal_version: Some(causal_version),
        yjs_update: None,
        note,
        attachment,
        clipboard,
        tag,
        note_tag,
    })
}

/// Build an envelope for cloud sync (per-change, retains seq).
fn build_envelope(
    db: &Database,
    change: &SyncChange,
    device_id: &str,
) -> Result<SyncEnvelope, String> {
    let causal_version = db
        .ensure_local_causal_version(&change.entity_type, &change.entity_id, device_id)
        .map_err(|e| e.to_string())?;

    let mut note = None;
    let mut attachment = None;
    let mut clipboard = None;
    let mut tag = None;
    let mut note_tag = None;

    match change.entity_type.as_str() {
        "note" => {
            note = db.get_note_for_sync(&change.entity_id).map_err(|e| e.to_string())?;
        }
        "attachment" => {
            attachment = db.get_attachment(&change.entity_id).map_err(|e| e.to_string())?;
        }
        "clipboard" => {
            clipboard = db
                .get_clipboard_item_for_sync(&change.entity_id)
                .map_err(|e| e.to_string())?;
        }
        "tag" => {
            tag = db.get_tag_for_sync(&change.entity_id).map_err(|e| e.to_string())?;
        }
        "note_tag" => {
            note_tag = db
                .get_note_tag_for_sync(&change.entity_id)
                .map_err(|e| e.to_string())?;
        }
        _ => {}
    }

    Ok(SyncEnvelope {
        schema_version: 2,
        device_id: device_id.to_string(),
        seq: change.seq,
        entity_type: change.entity_type.clone(),
        entity_id: change.entity_id.clone(),
        operation: change.operation.clone(),
        changed_at: change.changed_at.clone(),
        causal_version: Some(causal_version),
        yjs_update: None,
        note,
        attachment,
        clipboard,
        tag,
        note_tag,
    })
}

async fn pull_state(
    provider: &dyn SyncProvider,
    db: &Database,
    attachments_dir: &Path,
    config: &SyncConfig,
) -> Result<(usize, usize), String> {
    let mut pulled = 0;
    let mut conflicts = 0;

    // List remote device_ids under state/
    let device_ids = provider.list("state").await.unwrap_or_default();
    for device_id in &device_ids {
        if device_id == &config.device_id || !is_safe_path_segment(device_id) {
            continue;
        }
        // List entity types under state/{device_id}/
        let entity_types = provider
            .list(&format!("state/{device_id}"))
            .await
            .unwrap_or_default();
        for entity_type in &entity_types {
            if !is_safe_path_segment(entity_type) {
                continue;
            }
            // List entity files under state/{device_id}/{entity_type}/
            let files = provider
                .list(&format!("state/{device_id}/{entity_type}"))
                .await
                .unwrap_or_default();
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
                let legacy_fallback =
                    CausalVersion::legacy(&envelope.device_id, envelope.seq);
                let remote_version = envelope
                    .causal_version
                    .as_ref()
                    .unwrap_or(&legacy_fallback);
                let local_version = db
                    .get_entity_causal_version(entity_type, entity_id)
                    .map_err(|e| e.to_string())?;

                let attachment_missing = envelope.entity_type == "attachment"
                    && envelope
                        .attachment
                        .as_ref()
                        .is_some_and(|record| !local_attachment_available(db, attachments_dir, record));
                let should_apply = attachment_missing || match &local_version {
                    None => true, // no local version → apply
                    Some(local) => {
                        match remote_version.relation(local) {
                            CausalRelation::Dominates | CausalRelation::Concurrent => true,
                            CausalRelation::Equal | CausalRelation::Dominated => false,
                        }
                    }
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
    }
    Ok((pulled, conflicts))
}

fn local_attachment_available(
    db: &Database,
    attachments_dir: &Path,
    record: &AttachmentRecord,
) -> bool {
    db.get_attachment(&record.id)
        .ok()
        .flatten()
        .is_some_and(|local| attachments_dir.join(local.relative_path).exists())
}

fn is_safe_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
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

fn validate_envelope(
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
    use quicknote_protocol::Note;
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
            let immutable = path.starts_with("attachments/");
            if immutable {
                if let Some(existing) = objects.get(path) {
                    if existing != &body {
                        return Err(format!("immutable collision at {path}"));
                    }
                } else {
                    objects.insert(path.to_string(), body);
                }
            } else {
                objects.insert(path.to_string(), body);
            }
            if self.fail_after_next_put.swap(false, Ordering::SeqCst) {
                return Err("injected acknowledgement loss".to_string());
            }
            Ok(())
        }

        async fn delete(&self, path: &str) -> Result<(), String> {
            self.objects.lock().unwrap().remove(path);
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
            cloud_enabled: false,
            cloud_url: String::new(),
            cloud_email: String::new(),
            cloud_cursor_seq: 0,
            cloud_token_created_at: 0,
            password_salt: None,
            webdav_password_encrypted: None,
            cloud_password_encrypted: None,
            cloud_token_encrypted: None,
        }
    }

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
            push_state(&provider, &db, dir.path(), &sync_config).await.unwrap(),
            1
        );

        // Edit same note again
        db.update_note(&note.id, "<p>second</p>").unwrap();
        assert_eq!(
            push_state(&provider, &db, dir.path(), &sync_config).await.unwrap(),
            1
        );

        // Only one state file for this entity
        let state_files: Vec<String> = provider
            .objects
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with("state/"))
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
                .await.unwrap(), 1
        );

        // Device B pulls it
        assert_eq!(
            pull_state(&provider, &db_b, dir_b.path(), &config("device-b"))
                .await.unwrap(), (1, 0)
        );

        // Device B edits and pushes
        db_b.update_note(&note.id, "<p>edited by B</p>").unwrap();
        assert_eq!(
            push_state(&provider, &db_b, dir_b.path(), &config("device-b"))
                .await.unwrap(), 1
        );

        // Device A pulls B's edit — should apply (B dominates)
        let (pulled, _) = pull_state(&provider, &db_a, dir_a.path(), &config("device-a"))
            .await.unwrap();
        assert_eq!(pulled, 1);

        // Second pull without changes — should skip (equal)
        let (pulled2, _) = pull_state(&provider, &db_a, dir_a.path(), &config("device-a"))
            .await.unwrap();
        assert_eq!(pulled2, 0);
    }
}
