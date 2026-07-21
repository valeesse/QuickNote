mod cloud;
mod crypto;
mod pull;
mod push;
mod service;
mod v4;
mod webdav;

use crypto::*;
use pull::*;
use push::*;

use crate::db::{AttachmentRecord, Database, SyncChange};
use aes_gcm::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
#[cfg(test)]
use quicknote_protocol::CausalRelation;
use quicknote_protocol::CausalVersion;
pub use quicknote_protocol::SyncEnvelope;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Component;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;
use uuid::Uuid;
use webdav::WebDavProvider;

const PROVIDER_NAME: &str = "webdav";
#[cfg(test)]
const WEBDAV_BATCH_ENTITY_LIMIT: i64 = 100;
#[cfg(test)]
const WEBDAV_BATCH_BYTES_LIMIT: usize = 256 * 1024;
#[cfg(test)]
const ATTACHMENT_CHUNK_THRESHOLD: usize = 4 * 1024 * 1024;
#[cfg(test)]
const ATTACHMENT_CHUNK_SIZE: usize = 1024 * 1024;
const WEBDAV_SYNC_BUDGET: std::time::Duration = std::time::Duration::from_secs(60);

#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WebDavHead {
    schema_version: u32,
    device_id: String,
    generation: u64,
    batch_hash: String,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WebDavBatch {
    schema_version: u32,
    device_id: String,
    generation: u64,
    envelopes: Vec<SyncEnvelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttachmentChunkManifest {
    schema_version: u32,
    id: String,
    size: usize,
    chunk_size: usize,
    chunks: usize,
    sha256: String,
}

#[cfg(test)]
fn device_head_path(device_id: &str) -> String {
    format!("device-heads/{device_id}.json")
}

#[cfg(test)]
fn batch_path(device_id: &str, generation: u64) -> String {
    format!("changes/{device_id}/{generation:020}.json.gz")
}

fn attachment_manifest_path(id: &str) -> String {
    format!("attachment-manifests/{id}.json")
}

fn attachment_chunk_path(id: &str, index: usize) -> String {
    format!("attachment-chunks/{id}.{index:08}")
}

fn gzip_json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(&json)
        .map_err(|error| error.to_string())?;
    encoder.finish().map_err(|error| error.to_string())
}

fn gunzip_json<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, String> {
    let mut decoder = GzDecoder::new(bytes);
    let mut json = Vec::new();
    decoder
        .read_to_end(&mut json)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&json).map_err(|error| error.to_string())
}

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

#[derive(Debug, Serialize)]
pub struct WebDavStorageStatus {
    pub protocol_version: u32,
    pub workspace_id: String,
    pub epoch: u64,
    pub devices: usize,
    pub stored_objects: usize,
    pub reachable_objects: usize,
    pub pending_gc_objects: usize,
    pub stored_bytes: usize,
}

#[derive(Debug, Serialize)]
pub struct WebDavGcReport {
    pub deleted_objects: usize,
    pub status: WebDavStorageStatus,
}

#[async_trait]
pub trait SyncProvider: Send + Sync {
    async fn prepare(&self, device_id: &str) -> Result<(), String>;
    async fn ensure_collection(&self, _path: &str) -> Result<(), String> {
        Ok(())
    }
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

#[cfg(test)]
mod tests;
