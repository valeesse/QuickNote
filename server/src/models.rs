use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Note {
    pub id: String,
    pub user_id: Uuid,
    pub title: String,
    pub content: String,
    pub is_pinned: bool,
    pub created_at: String,
    pub updated_at: String,
    pub version: i32,
    pub is_deleted: bool,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct NoteSummary {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub is_pinned: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateNoteRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNoteRequest {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ClipboardItem {
    pub id: String,
    pub user_id: Uuid,
    pub kind: String,
    pub content: String,
    pub preview: String,
    pub source_device: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_copied_at: String,
    pub capture_count: i32,
    pub is_pinned: bool,
    pub is_deleted: bool,
}

#[derive(Debug, Deserialize)]
pub struct CaptureClipboardRequest {
    pub content: String,
    pub kind: Option<String>,
    pub source_device: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalVersion {
    pub device_id: String,
    pub seq: i64,
    pub deps: Vec<(String, i64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncEnvelope {
    pub schema_version: u32,
    pub device_id: String,
    pub seq: i64,
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
    pub changed_at: String,
    pub causal_version: Option<CausalVersion>,
    pub note: Option<Note>,
    pub clipboard: Option<ClipboardItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncEvent {
    pub user_id: Uuid,
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
}

#[derive(Debug, Deserialize)]
pub struct PullRequest {
    pub since_seq: i64,
}

#[derive(Debug, Serialize)]
pub struct PullResponse {
    pub envelopes: Vec<SyncEnvelope>,
    pub server_seq: i64,
}

#[derive(Debug, Deserialize)]
pub struct PushRequest {
    pub envelopes: Vec<SyncEnvelope>,
}

#[derive(Debug, Serialize)]
pub struct PushResponse {
    pub accepted: usize,
    pub conflicts: usize,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct CloudChange {
    pub id: i64,
    pub user_id: Uuid,
    pub seq: i64,
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
    pub envelope: serde_json::Value,
    pub created_at: DateTime<Utc>,
}
