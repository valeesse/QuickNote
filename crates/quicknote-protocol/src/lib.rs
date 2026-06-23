use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub is_pinned: bool,
    pub created_at: String,
    pub updated_at: String,
    pub version: i64,
    pub is_deleted: bool,
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
    pub note: Option<Note>,
    pub attachment: Option<AttachmentRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clipboard: Option<ClipboardItem>,
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
}
