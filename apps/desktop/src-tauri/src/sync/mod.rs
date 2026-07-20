mod cloud;
mod crypto;
mod pull;
mod push;
mod service;
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
pub use quicknote_protocol::SyncEnvelope;
use quicknote_protocol::{CausalRelation, CausalVersion};
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
