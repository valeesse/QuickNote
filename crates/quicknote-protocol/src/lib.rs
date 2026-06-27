use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yjs_state: Option<Vec<u8>>,
    #[serde(default)]
    pub yjs_state_version: i64,
    pub is_pinned: bool,
    #[serde(default)]
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
    pub version: i64,
    pub is_deleted: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct NoteSummary {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub is_pinned: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub normalized_name: String,
    pub color: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct TagSummary {
    pub id: String,
    pub name: String,
    pub normalized_name: String,
    pub color: Option<String>,
    pub note_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct NoteTag {
    pub id: String,
    pub note_id: String,
    pub tag_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct NoteVersion {
    pub id: i64,
    pub note_id: String,
    pub title: String,
    pub content: String,
    pub version: i64,
    pub created_at: String,
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct AttachmentRecord {
    pub id: String,
    pub relative_path: String,
    pub mime_type: String,
    pub size: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct ClipboardItem {
    pub id: String,
    pub kind: String,
    pub content: String,
    pub preview: String,
    pub source_device: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_copied_at: String,
    pub capture_count: i64,
    pub is_pinned: bool,
    pub is_deleted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CausalVersion {
    #[serde(default)]
    pub counters: BTreeMap<String, u64>,
    #[serde(default)]
    pub origin: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CausalRelation {
    Equal,
    Dominates,
    Dominated,
    Concurrent,
}

impl CausalVersion {
    pub fn legacy(device_id: &str, sequence: i64) -> Self {
        Self {
            counters: BTreeMap::from([(device_id.to_string(), sequence.max(1) as u64)]),
            origin: device_id.to_string(),
        }
    }

    pub fn server(sequence: i64) -> Self {
        Self::legacy("cloud", sequence)
    }

    pub fn increment(&mut self, device_id: &str) {
        *self.counters.entry(device_id.to_string()).or_default() += 1;
        self.origin = device_id.to_string();
    }

    pub fn relation(&self, other: &Self) -> CausalRelation {
        let mut greater = false;
        let mut less = false;
        for device in self.counters.keys().chain(other.counters.keys()) {
            let left = self.counters.get(device).copied().unwrap_or(0);
            let right = other.counters.get(device).copied().unwrap_or(0);
            greater |= left > right;
            less |= left < right;
        }
        match (greater, less) {
            (false, false) => CausalRelation::Equal,
            (true, false) => CausalRelation::Dominates,
            (false, true) => CausalRelation::Dominated,
            (true, true) => CausalRelation::Concurrent,
        }
    }

    pub fn merge_with_winner(&self, other: &Self, winner: &Self) -> Self {
        let mut counters = self.counters.clone();
        for (device, value) in &other.counters {
            let current = counters.entry(device.clone()).or_default();
            *current = (*current).max(*value);
        }
        Self {
            counters,
            origin: winner.origin.clone(),
        }
    }

    pub fn deterministic_cmp(&self, other: &Self) -> Ordering {
        self.origin
            .cmp(&other.origin)
            .then_with(|| self.counters.cmp(&other.counters))
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causal_version: Option<CausalVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yjs_update: Option<Vec<u8>>,
    pub note: Option<Note>,
    pub attachment: Option<AttachmentRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clipboard: Option<ClipboardItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<Tag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_tag: Option<NoteTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct YjsHead {
    pub schema_version: u32,
    pub note_id: String,
    pub snapshot: i64,
    pub latest_update: i64,
    pub state_version: i64,
    pub writer_device: String,
}

impl YjsHead {
    pub fn new(note_id: impl Into<String>, writer_device: impl Into<String>) -> Self {
        Self {
            schema_version: 3,
            note_id: note_id.into(),
            snapshot: 0,
            latest_update: 0,
            state_version: 0,
            writer_device: writer_device.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct BillingPlan {
    pub id: String,
    pub name: String,
    pub tier: String,
    pub description: String,
    pub cloud_enabled: bool,
    pub version_history_days: Option<i32>,
    pub max_devices: Option<i32>,
    pub max_attachment_bytes: i64,
    pub sync_priority: String,
    pub checkout_cta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct BillingPrice {
    pub id: String,
    pub plan_id: String,
    pub provider: String,
    pub provider_price_id: String,
    pub billing_interval: String,
    pub currency: String,
    pub unit_amount: i32,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct SubscriptionSummary {
    pub plan_id: String,
    pub price_id: String,
    pub provider: String,
    pub status: String,
    pub cancel_at_period_end: bool,
    pub current_period_start: Option<String>,
    pub current_period_end: Option<String>,
    pub management_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitlementSummary {
    pub key: String,
    pub enabled: bool,
    pub limit: Option<i64>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMetric {
    pub key: String,
    pub used: i64,
    pub limit: Option<i64>,
    pub unit: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn causal_relation_detects_concurrent_versions() {
        let left = CausalVersion {
            counters: BTreeMap::from([("a".into(), 1)]),
            origin: "a".into(),
        };
        let right = CausalVersion {
            counters: BTreeMap::from([("b".into(), 1)]),
            origin: "b".into(),
        };
        assert_eq!(left.relation(&right), CausalRelation::Concurrent);
    }

    #[test]
    fn yjs_head_defaults_to_schema_three_empty_cursors() {
        let head = YjsHead::new("note-a", "device-a");
        assert_eq!(head.schema_version, 3);
        assert_eq!(head.note_id, "note-a");
        assert_eq!(head.snapshot, 0);
        assert_eq!(head.latest_update, 0);
        assert_eq!(head.state_version, 0);
        assert_eq!(head.writer_device, "device-a");
    }
}
