use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use quicknote_protocol::{
    AttachmentRecord, BillingPlan, BillingPrice, CausalVersion, ClipboardItem, EntitlementSummary,
    Note, NoteSummary, NoteTag, NoteVersion, SubscriptionSummary, SyncEnvelope, Tag, TagSummary,
    UsageMetric,
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

#[derive(Debug, Serialize)]
pub struct AccountSummary {
    pub user: UserResponse,
    pub plans: Vec<BillingPlan>,
    pub prices: Vec<BillingPrice>,
    pub subscription: Option<SubscriptionSummary>,
    pub entitlements: Vec<EntitlementSummary>,
    pub usage: Vec<UsageMetric>,
    pub billing_provider: Option<String>,
    pub billing_ready: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateCheckoutRequest {
    pub price_id: String,
}

#[derive(Debug, Serialize)]
pub struct CheckoutSessionResponse {
    pub checkout_url: String,
}

#[derive(Debug, Serialize)]
pub struct BillingPortalResponse {
    pub management_url: String,
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

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SubscriptionRecord {
    pub plan_id: String,
    pub price_id: String,
    pub provider: String,
    pub status: String,
    pub cancel_at_period_end: bool,
    pub current_period_start: Option<DateTime<Utc>>,
    pub current_period_end: Option<DateTime<Utc>>,
    pub management_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LemonSqueezyWebhookPayload {
    pub meta: serde_json::Value,
    pub data: serde_json::Value,
}
