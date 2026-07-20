use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use quicknote_protocol::{
    AttachmentRecord, CausalRelation, CausalVersion, ClipboardItem, Note, NoteSummary, NoteTag,
    NoteVersion, SyncEnvelope, Tag, TagSummary,
};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Deserialize)]
pub struct CreateNoteRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNoteRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNoteTagsRequest {
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReorderNotesRequest {
    pub ids: Vec<String>,
    pub is_pinned: bool,
}

#[derive(Debug, Deserialize)]
pub struct CaptureClipboardRequest {
    pub content: String,
    pub kind: Option<String>,
    pub source_device: Option<String>,
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
    pub acknowledged_sequences: Vec<i64>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct CloudChange {
    pub id: i64,
    pub user_id: Uuid,
    pub seq: i64,
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
    pub source_device: String,
    pub source_seq: i64,
    pub envelope: serde_json::Value,
    pub created_at: DateTime<Utc>,
}
