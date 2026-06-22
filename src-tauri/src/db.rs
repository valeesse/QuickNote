use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

const VERSION_SNAPSHOT_INTERVAL_SECONDS: i64 = 5 * 60;
const UNPINNED_VERSION_LIMIT: i64 = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct NoteSummary {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub is_pinned: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct AttachmentRecord {
    pub id: String,
    pub relative_path: String,
    pub mime_type: String,
    pub size: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncChange {
    pub seq: i64,
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
    pub changed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let mut counters = BTreeMap::new();
        counters.insert(device_id.to_string(), sequence.max(1) as u64);
        Self {
            counters,
            origin: device_id.to_string(),
        }
    }

    fn increment(&mut self, device_id: &str) {
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

    fn merge_with_winner(&self, other: &Self, winner: &Self) -> Self {
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

    fn deterministic_cmp(&self, other: &Self) -> Ordering {
        self.origin
            .cmp(&other.origin)
            .then_with(|| self.counters.cmp(&other.counters))
    }
}

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new(app_dir: PathBuf) -> Result<Self> {
        let db_path = app_dir.join("notes.db");
        let conn = Connection::open(db_path)?;

        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA synchronous=NORMAL;")?;
        conn.execute_batch("PRAGMA cache_size=-64000;")?; // 64MB cache

        // Create tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS notes (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL DEFAULT '',
                plain_text TEXT NOT NULL DEFAULT '',
                preview TEXT NOT NULL DEFAULT '',
                is_pinned INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                is_deleted INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;

        ensure_column(&conn, "notes", "plain_text", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "notes", "preview", "TEXT NOT NULL DEFAULT ''")?;

        let schema_version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if schema_version < 2 {
            let mut stmt = conn.prepare(
                "SELECT id, content FROM notes WHERE content != '' AND (plain_text = '' OR preview = '')",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>>>()?;
            drop(stmt);

            for (id, content) in rows {
                let plain_text = html_to_text(&content);
                conn.execute(
                    "UPDATE notes SET plain_text = ?1, preview = ?2 WHERE id = ?3",
                    params![plain_text, make_preview_without_title(&content), id],
                )?;
            }
        }

        conn.execute(
            "CREATE TABLE IF NOT EXISTS note_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                note_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                version INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                is_pinned INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;
        ensure_column(
            &conn,
            "note_versions",
            "is_pinned",
            "INTEGER NOT NULL DEFAULT 0",
        )?;

        // Create FTS5 virtual table for full-text search
        conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
                title,
                plain_text,
                content='notes',
                content_rowid='rowid'
            );",
        )?;

        // Triggers to keep FTS index in sync
        conn.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS notes_ai AFTER INSERT ON notes BEGIN
                INSERT INTO notes_fts(rowid, title, plain_text) VALUES (new.rowid, new.title, new.plain_text);
            END;
            CREATE TRIGGER IF NOT EXISTS notes_ad AFTER DELETE ON notes BEGIN
                INSERT INTO notes_fts(notes_fts, rowid, title, plain_text) VALUES('delete', old.rowid, old.title, old.plain_text);
            END;
            CREATE TRIGGER IF NOT EXISTS notes_au AFTER UPDATE ON notes BEGIN
                INSERT INTO notes_fts(notes_fts, rowid, title, plain_text) VALUES('delete', old.rowid, old.title, old.plain_text);
                INSERT INTO notes_fts(rowid, title, plain_text) VALUES (new.rowid, new.title, new.plain_text);
            END;"
        )?;

        if schema_version < 2 {
            conn.execute("INSERT INTO notes_fts(notes_fts) VALUES('rebuild')", [])?;
        }

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS attachments (
                id TEXT PRIMARY KEY,
                relative_path TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                size INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sync_changes (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                operation TEXT NOT NULL,
                changed_at TEXT NOT NULL,
                synced INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS sync_cursors (
                provider TEXT NOT NULL,
                device_id TEXT NOT NULL,
                cursor TEXT NOT NULL,
                PRIMARY KEY(provider, device_id)
            );
            CREATE TABLE IF NOT EXISTS note_tombstones (
                id TEXT PRIMARY KEY,
                deleted_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sync_bootstrap (
                scope TEXT PRIMARY KEY,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sync_entity_versions (
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                version_json TEXT NOT NULL,
                dirty INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(entity_type, entity_id)
            );
            CREATE TABLE IF NOT EXISTS clipboard_items (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                content TEXT NOT NULL,
                preview TEXT NOT NULL,
                source_device TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_copied_at TEXT NOT NULL,
                capture_count INTEGER NOT NULL DEFAULT 1,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                is_deleted INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_sync_changes_pending
                ON sync_changes(synced, seq);
            CREATE INDEX IF NOT EXISTS idx_clipboard_recent
                ON clipboard_items(is_deleted, is_pinned DESC, last_copied_at DESC);",
        )?;

        conn.pragma_update(None, "user_version", 5)?;

        // Create index on updated_at for sync
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated_at)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_notes_pinned ON notes(is_pinned, updated_at DESC)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_note_versions_note ON note_versions(note_id, version DESC)",
            [],
        )?;

        Ok(Database {
            conn: Mutex::new(conn),
        })
    }

    pub fn create_note(&self, content: &str) -> Result<Note> {
        let mut conn = self.conn.lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let title = extract_title(content);
        let plain_text = html_to_text(content);
        let preview = make_preview_without_title(content);
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO notes (id, title, content, plain_text, preview, is_pinned, created_at, updated_at, version, is_deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, 1, 0)",
            params![id, title, content, plain_text, preview, now, now],
        )?;
        enqueue_change(&tx, "note", &id, "upsert", &now)?;
        tx.commit()?;

        Ok(Note {
            id,
            title,
            content: content.to_string(),
            is_pinned: false,
            created_at: now.clone(),
            updated_at: now,
            version: 1,
            is_deleted: false,
        })
    }

    pub fn get_note(&self, id: &str) -> Result<Option<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, content, is_pinned, created_at, updated_at, version, is_deleted
             FROM notes WHERE id = ?1 AND is_deleted = 0",
        )?;

        let note = stmt
            .query_row(params![id], |row| {
                Ok(Note {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    is_pinned: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    version: row.get(6)?,
                    is_deleted: row.get(7)?,
                })
            })
            .optional()?;

        Ok(note)
    }

    pub fn list_notes(&self) -> Result<Vec<NoteSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, preview, is_pinned, created_at, updated_at
             FROM notes
             WHERE is_deleted = 0
             ORDER BY is_pinned DESC, updated_at DESC",
        )?;

        let notes = stmt
            .query_map([], |row| {
                Ok(NoteSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    preview: row.get(2)?,
                    is_pinned: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        Ok(notes)
    }

    pub fn update_note(&self, id: &str, content: &str) -> Result<Option<Note>> {
        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let title = extract_title(content);
        let plain_text = html_to_text(content);
        let preview = make_preview_without_title(content);
        let tx = conn.transaction()?;

        self.snapshot_note_if_due_locked(&tx, id, &now)?;

        let rows = tx.execute(
            "UPDATE notes SET title = ?1, content = ?2, plain_text = ?3, preview = ?4,
                              updated_at = ?5, version = version + 1
             WHERE id = ?6 AND is_deleted = 0",
            params![title, content, plain_text, preview, now, id],
        )?;

        if rows == 0 {
            return Ok(None);
        }

        enqueue_change(&tx, "note", id, "upsert", &now)?;
        let note = get_note_locked(&tx, id, false)?;
        tx.commit()?;
        Ok(note)
    }

    pub fn delete_note(&self, id: &str) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;

        let rows = tx.execute(
            "UPDATE notes SET is_deleted = 1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        if rows > 0 {
            enqueue_change(&tx, "note", id, "delete", &now)?;
        }
        tx.commit()?;

        Ok(rows > 0)
    }

    pub fn restore_note(&self, id: &str) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;

        let rows = tx.execute(
            "UPDATE notes SET is_deleted = 0, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        if rows > 0 {
            enqueue_change(&tx, "note", id, "upsert", &now)?;
        }
        tx.commit()?;

        Ok(rows > 0)
    }

    pub fn purge_note(&self, id: &str) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;
        let rows = tx.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        tx.execute("DELETE FROM note_versions WHERE note_id = ?1", params![id])?;
        if rows > 0 {
            tx.execute(
                "INSERT INTO note_tombstones(id, deleted_at) VALUES(?1, ?2)
                 ON CONFLICT(id) DO UPDATE SET deleted_at = excluded.deleted_at",
                params![id, now],
            )?;
            enqueue_change(&tx, "note", id, "delete", &now)?;
        }
        tx.commit()?;
        Ok(rows > 0)
    }

    pub fn list_deleted_notes(&self) -> Result<Vec<NoteSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, preview, is_pinned, created_at, updated_at
             FROM notes
             WHERE is_deleted = 1
             ORDER BY updated_at DESC",
        )?;

        let notes = stmt
            .query_map([], |row| {
                Ok(NoteSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    preview: row.get(2)?,
                    is_pinned: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        Ok(notes)
    }

    pub fn toggle_pin(&self, id: &str) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;

        let rows = tx.execute(
            "UPDATE notes SET is_pinned = NOT is_pinned, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        if rows > 0 {
            enqueue_change(&tx, "note", id, "upsert", &now)?;
        }
        tx.commit()?;

        Ok(rows > 0)
    }

    pub fn search_notes(&self, query: &str) -> Result<Vec<NoteSummary>> {
        let conn = self.conn.lock().unwrap();
        let normalized_query = normalize_fts_query(query);
        if normalized_query.is_empty() {
            drop(conn);
            return self.list_notes();
        }

        let mut stmt = conn.prepare(
            "SELECT n.id, n.title, n.preview,
                    n.is_pinned, n.created_at, n.updated_at
             FROM notes n
             JOIN notes_fts fts ON n.rowid = fts.rowid
             WHERE notes_fts MATCH ?1 AND n.is_deleted = 0
             ORDER BY rank
             LIMIT 50",
        )?;

        let notes = stmt
            .query_map(params![normalized_query], |row| {
                Ok(NoteSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    preview: row.get(2)?,
                    is_pinned: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        Ok(notes)
    }

    pub fn get_note_versions(&self, id: &str) -> Result<Vec<NoteVersion>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, note_id, title, content, version, created_at, is_pinned
             FROM note_versions
             WHERE note_id = ?1
             ORDER BY is_pinned DESC, version DESC
             LIMIT 50",
        )?;

        let versions = stmt
            .query_map(params![id], |row| {
                Ok(NoteVersion {
                    id: row.get(0)?,
                    note_id: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    version: row.get(4)?,
                    created_at: row.get(5)?,
                    is_pinned: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        Ok(versions)
    }

    pub fn toggle_version_pin(&self, version_id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "UPDATE note_versions SET is_pinned = NOT is_pinned WHERE id = ?1",
            params![version_id],
        )?;
        Ok(rows > 0)
    }

    pub fn restore_note_version(&self, note_id: &str, version_id: i64) -> Result<Option<Note>> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let version_content = tx
            .query_row(
                "SELECT content FROM note_versions WHERE id = ?1 AND note_id = ?2",
                params![version_id, note_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        let Some(version_content) = version_content else {
            return Ok(None);
        };

        let current = tx
            .query_row(
                "SELECT id, title, content, version FROM notes WHERE id = ?1 AND is_deleted = 0",
                params![note_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;

        let Some((id, title, content, version)) = current else {
            return Ok(None);
        };

        let now = Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO note_versions (note_id, title, content, version, created_at, is_pinned)
             VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![id, title, content, version, now],
        )?;

        let restored_title = extract_title(&version_content);
        let plain_text = html_to_text(&version_content);
        let preview = make_preview_without_title(&version_content);
        tx.execute(
            "UPDATE notes SET title = ?1, content = ?2, plain_text = ?3, preview = ?4,
                              updated_at = ?5, version = version + 1
             WHERE id = ?6 AND is_deleted = 0",
            params![
                restored_title,
                version_content,
                plain_text,
                preview,
                now,
                note_id
            ],
        )?;

        prune_unpinned_versions(&tx, note_id, UNPINNED_VERSION_LIMIT)?;
        enqueue_change(&tx, "note", note_id, "upsert", &now)?;
        let note = get_note_locked(&tx, note_id, false)?;
        tx.commit()?;
        Ok(note)
    }

    fn snapshot_note_if_due_locked(&self, conn: &Connection, id: &str, now: &str) -> Result<()> {
        let current = conn
            .query_row(
                "SELECT title, content, version FROM notes WHERE id = ?1 AND is_deleted = 0",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;

        let Some((title, content, version)) = current else {
            return Ok(());
        };

        if html_to_text(&content).is_empty() {
            return Ok(());
        }

        let latest = conn
            .query_row(
                "SELECT content, created_at FROM note_versions WHERE note_id = ?1 ORDER BY created_at DESC LIMIT 1",
                params![id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        if let Some((latest_content, latest_created_at)) = latest {
            if latest_content == content {
                return Ok(());
            }

            let latest_time = chrono::DateTime::parse_from_rfc3339(&latest_created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .ok();
            let now_time = chrono::DateTime::parse_from_rfc3339(now)
                .map(|dt| dt.with_timezone(&Utc))
                .ok();

            if let (Some(latest_time), Some(now_time)) = (latest_time, now_time) {
                if (now_time - latest_time).num_seconds() < VERSION_SNAPSHOT_INTERVAL_SECONDS {
                    return Ok(());
                }
            }
        }

        conn.execute(
            "INSERT INTO note_versions (note_id, title, content, version, created_at, is_pinned)
             VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![id, title, content, version, now],
        )?;

        prune_unpinned_versions(conn, id, UNPINNED_VERSION_LIMIT)?;

        Ok(())
    }

    pub fn register_attachment(
        &self,
        id: &str,
        relative_path: &str,
        mime_type: &str,
        size: i64,
    ) -> Result<AttachmentRecord> {
        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO attachments(id, relative_path, mime_type, size, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO NOTHING",
            params![id, relative_path, mime_type, size, now],
        )?;
        enqueue_change(&tx, "attachment", id, "upsert", &now)?;
        let record = tx.query_row(
            "SELECT id, relative_path, mime_type, size, created_at FROM attachments WHERE id = ?1",
            params![id],
            attachment_from_row,
        )?;
        tx.commit()?;
        Ok(record)
    }

    pub fn get_attachment(&self, id: &str) -> Result<Option<AttachmentRecord>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, relative_path, mime_type, size, created_at FROM attachments WHERE id = ?1",
            params![id],
            attachment_from_row,
        )
        .optional()
    }

    pub fn list_pending_changes(&self, limit: i64) -> Result<Vec<SyncChange>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT seq, entity_type, entity_id, operation, changed_at
             FROM sync_changes WHERE synced = 0 ORDER BY seq LIMIT ?1",
        )?;
        let changes = stmt
            .query_map(params![limit], |row| {
                Ok(SyncChange {
                    seq: row.get(0)?,
                    entity_type: row.get(1)?,
                    entity_id: row.get(2)?,
                    operation: row.get(3)?,
                    changed_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;
        Ok(changes)
    }

    pub fn ensure_sync_bootstrap(&self, scope: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_bootstrap WHERE scope = ?1)",
            params![scope],
            |row| row.get(0),
        )?;
        if exists {
            return Ok(());
        }

        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO sync_changes(entity_type, entity_id, operation, changed_at, synced)
             SELECT 'note', id, CASE WHEN is_deleted = 1 THEN 'delete' ELSE 'upsert' END, ?1, 0
             FROM notes",
            params![now],
        )?;
        tx.execute(
            "INSERT INTO sync_changes(entity_type, entity_id, operation, changed_at, synced)
             SELECT 'attachment', id, 'upsert', ?1, 0 FROM attachments",
            params![now],
        )?;
        tx.execute(
            "INSERT INTO sync_changes(entity_type, entity_id, operation, changed_at, synced)
             SELECT 'clipboard', id, CASE WHEN is_deleted = 1 THEN 'delete' ELSE 'upsert' END, ?1, 0
             FROM clipboard_items",
            params![now],
        )?;
        tx.execute(
            "INSERT INTO sync_bootstrap(scope, created_at) VALUES(?1, ?2)",
            params![scope, now],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn mark_change_synced(&self, seq: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sync_changes SET synced = 1 WHERE seq = ?1",
            params![seq],
        )?;
        Ok(())
    }

    pub fn get_sync_cursor(&self, provider: &str, device_id: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let cursor = conn
            .query_row(
                "SELECT cursor FROM sync_cursors WHERE provider = ?1 AND device_id = ?2",
                params![provider, device_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(cursor.and_then(|value| value.parse().ok()).unwrap_or(0))
    }

    pub fn set_sync_cursor(&self, provider: &str, device_id: &str, cursor: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sync_cursors(provider, device_id, cursor) VALUES(?1, ?2, ?3)
             ON CONFLICT(provider, device_id) DO UPDATE SET cursor = excluded.cursor",
            params![provider, device_id, cursor.to_string()],
        )?;
        Ok(())
    }

    pub fn get_note_for_sync(&self, id: &str) -> Result<Option<Note>> {
        let conn = self.conn.lock().unwrap();
        get_note_locked(&conn, id, true)
    }

    pub fn ensure_local_causal_version(
        &self,
        entity_type: &str,
        entity_id: &str,
        device_id: &str,
    ) -> Result<CausalVersion> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let version = ensure_local_causal_version_locked(&tx, entity_type, entity_id, device_id)?;
        tx.commit()?;
        Ok(version)
    }

    pub fn apply_remote_note(
        &self,
        remote: &Note,
        remote_version: &CausalVersion,
        local_device_id: &str,
    ) -> Result<(bool, bool)> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let local = get_note_locked(&tx, &remote.id, true)?;
        let local_dirty: bool = tx.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sync_changes
                WHERE synced = 0 AND entity_type = 'note' AND entity_id = ?1
            )",
            params![remote.id],
            |row| row.get(0),
        )?;
        let mut local_version = get_entity_version_locked(&tx, "note", &remote.id)?;
        if local_dirty {
            local_version = Some(ensure_local_causal_version_locked(
                &tx,
                "note",
                &remote.id,
                local_device_id,
            )?);
        }

        let mut conflict = false;
        let version_to_store = if let Some(local_version) = &local_version {
            match local_version.relation(remote_version) {
                CausalRelation::Equal | CausalRelation::Dominates => {
                    tx.commit()?;
                    return Ok((false, false));
                }
                CausalRelation::Dominated => remote_version.clone(),
                CausalRelation::Concurrent => {
                    conflict = local
                        .as_ref()
                        .is_some_and(|note| note.is_deleted || note.content != remote.content);
                    let remote_wins = remote_version.deterministic_cmp(local_version).is_gt();
                    let winner = if remote_wins {
                        remote_version
                    } else {
                        local_version
                    };
                    let merged = local_version.merge_with_winner(remote_version, winner);
                    if conflict {
                        if remote_wins {
                            if let Some(local_note) = local.as_ref().filter(|note| !note.is_deleted)
                            {
                                insert_conflict_copy(&tx, local_note, local_version)?;
                            }
                        } else if !remote.is_deleted {
                            insert_conflict_copy(&tx, remote, remote_version)?;
                        }
                    }
                    if !remote_wins {
                        set_entity_version_locked(&tx, "note", &remote.id, &merged, false)?;
                        tx.commit()?;
                        return Ok((false, conflict));
                    }
                    merged
                }
            }
        } else {
            remote_version.clone()
        };

        upsert_remote_note_locked(&tx, remote)?;
        set_entity_version_locked(&tx, "note", &remote.id, &version_to_store, false)?;
        tx.commit()?;
        Ok((true, conflict))
    }

    pub fn apply_remote_delete(
        &self,
        id: &str,
        deleted_at: &str,
        remote_version: &CausalVersion,
        local_device_id: &str,
    ) -> Result<(bool, bool)> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let local = get_note_locked(&tx, id, true)?;
        let local_dirty: bool = tx.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sync_changes
                WHERE synced = 0 AND entity_type = 'note' AND entity_id = ?1
            )",
            params![id],
            |row| row.get(0),
        )?;
        let mut local_version = get_entity_version_locked(&tx, "note", id)?;
        if local_dirty {
            local_version = Some(ensure_local_causal_version_locked(
                &tx,
                "note",
                id,
                local_device_id,
            )?);
        }
        let mut conflict = false;
        let version_to_store = if let Some(local_version) = &local_version {
            match local_version.relation(remote_version) {
                CausalRelation::Equal | CausalRelation::Dominates => {
                    tx.commit()?;
                    return Ok((false, false));
                }
                CausalRelation::Dominated => remote_version.clone(),
                CausalRelation::Concurrent => {
                    conflict = local.as_ref().is_some_and(|note| !note.is_deleted);
                    let remote_wins = remote_version.deterministic_cmp(local_version).is_gt();
                    let winner = if remote_wins {
                        remote_version
                    } else {
                        local_version
                    };
                    let merged = local_version.merge_with_winner(remote_version, winner);
                    if remote_wins {
                        if let Some(local_note) = local.as_ref().filter(|note| !note.is_deleted) {
                            insert_conflict_copy(&tx, local_note, local_version)?;
                        }
                    } else {
                        set_entity_version_locked(&tx, "note", id, &merged, false)?;
                        tx.commit()?;
                        return Ok((false, conflict));
                    }
                    merged
                }
            }
        } else {
            remote_version.clone()
        };
        tx.execute(
            "UPDATE notes SET is_deleted = 1, updated_at = ?1 WHERE id = ?2",
            params![deleted_at, id],
        )?;
        tx.execute(
            "INSERT INTO note_tombstones(id, deleted_at) VALUES(?1, ?2)
             ON CONFLICT(id) DO UPDATE SET deleted_at = excluded.deleted_at",
            params![id, deleted_at],
        )?;
        set_entity_version_locked(&tx, "note", id, &version_to_store, false)?;
        tx.commit()?;
        Ok((true, conflict))
    }

    pub fn register_remote_attachment(&self, record: &AttachmentRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO attachments(id, relative_path, mime_type, size, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                relative_path = excluded.relative_path,
                mime_type = excluded.mime_type,
                size = excluded.size",
            params![
                record.id,
                record.relative_path,
                record.mime_type,
                record.size,
                record.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn orphan_attachments(&self) -> Result<Vec<AttachmentRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut referenced_content = String::new();
        {
            let mut stmt = conn
                .prepare("SELECT content FROM notes UNION ALL SELECT content FROM note_versions")?;
            let contents = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>>>()?;
            for content in contents {
                referenced_content.push_str(&content);
            }
        }

        let mut stmt =
            conn.prepare("SELECT id, relative_path, mime_type, size, created_at FROM attachments")?;
        let records = stmt
            .query_map([], attachment_from_row)?
            .collect::<Result<Vec<_>>>()?;
        Ok(records
            .into_iter()
            .filter(|record| !referenced_content.contains(&format!("attachment://{}", record.id)))
            .collect())
    }

    pub fn remove_attachment_record(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM attachments WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn capture_clipboard(&self, content: &str, source_device: &str) -> Result<ClipboardItem> {
        let normalized = normalize_clipboard_content(content);
        if normalized.trim().is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "clipboard content is empty".to_string(),
            ));
        }
        if normalized.len() > 1_000_000 {
            return Err(rusqlite::Error::InvalidParameterName(
                "clipboard content exceeds 1 MB".to_string(),
            ));
        }
        let id = format!(
            "{:x}",
            Sha256::digest(format!("clipboard:text:{normalized}"))
        );
        let kind = classify_clipboard_content(&normalized);
        let preview = truncate_chars(&normalized.replace('\n', " "), 240);
        let now = Utc::now().to_rfc3339();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO clipboard_items(
                id, kind, content, preview, source_device, created_at, updated_at,
                last_copied_at, capture_count, is_pinned, is_deleted
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?6, ?6, 1, 0, 0)
             ON CONFLICT(id) DO UPDATE SET
                source_device = excluded.source_device,
                updated_at = excluded.updated_at,
                last_copied_at = excluded.last_copied_at,
                capture_count = clipboard_items.capture_count + 1,
                is_deleted = 0",
            params![id, kind, normalized, preview, source_device, now],
        )?;
        enqueue_change(&tx, "clipboard", &id, "upsert", &now)?;
        let item = get_clipboard_item_locked(&tx, &id, true)?.expect("inserted clipboard item");
        tx.commit()?;
        Ok(item)
    }

    pub fn list_clipboard_items(&self, query: &str, limit: i64) -> Result<Vec<ClipboardItem>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", query.trim());
        let mut stmt = conn.prepare(
            "SELECT id, kind, content, preview, source_device, created_at, updated_at,
                    last_copied_at, capture_count, is_pinned, is_deleted
             FROM clipboard_items
             WHERE is_deleted = 0 AND (?1 = '%%' OR content LIKE ?1)
             ORDER BY is_pinned DESC, last_copied_at DESC
             LIMIT ?2",
        )?;
        let items = stmt
            .query_map(
                params![pattern, limit.clamp(1, 500)],
                clipboard_item_from_row,
            )?
            .collect();
        items
    }

    pub fn get_clipboard_item(&self, id: &str) -> Result<Option<ClipboardItem>> {
        let conn = self.conn.lock().unwrap();
        get_clipboard_item_locked(&conn, id, false)
    }

    pub fn get_clipboard_item_for_sync(&self, id: &str) -> Result<Option<ClipboardItem>> {
        let conn = self.conn.lock().unwrap();
        get_clipboard_item_locked(&conn, id, true)
    }

    pub fn toggle_clipboard_pin(&self, id: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE clipboard_items
             SET is_pinned = NOT is_pinned, updated_at = ?1
             WHERE id = ?2 AND is_deleted = 0",
            params![now, id],
        )?;
        if changed > 0 {
            enqueue_change(&tx, "clipboard", id, "upsert", &now)?;
        }
        tx.commit()?;
        Ok(changed > 0)
    }

    pub fn delete_clipboard_item(&self, id: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE clipboard_items SET is_deleted = 1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        if changed > 0 {
            enqueue_change(&tx, "clipboard", id, "delete", &now)?;
        }
        tx.commit()?;
        Ok(changed > 0)
    }

    pub fn touch_clipboard_item(&self, id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE clipboard_items SET last_copied_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        if changed > 0 {
            enqueue_change(&tx, "clipboard", id, "upsert", &now)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn apply_remote_clipboard(
        &self,
        remote: &ClipboardItem,
        remote_version: &CausalVersion,
        local_device_id: &str,
    ) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let local_dirty: bool = tx.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sync_changes
                WHERE synced = 0 AND entity_type = 'clipboard' AND entity_id = ?1
            )",
            params![remote.id],
            |row| row.get(0),
        )?;
        let mut local_version = get_entity_version_locked(&tx, "clipboard", &remote.id)?;
        if local_dirty {
            local_version = Some(ensure_local_causal_version_locked(
                &tx,
                "clipboard",
                &remote.id,
                local_device_id,
            )?);
        }
        let version_to_store = if let Some(local_version) = &local_version {
            match local_version.relation(remote_version) {
                CausalRelation::Equal | CausalRelation::Dominates => {
                    tx.commit()?;
                    return Ok(false);
                }
                CausalRelation::Dominated => remote_version.clone(),
                CausalRelation::Concurrent => {
                    let remote_wins = remote_version.deterministic_cmp(local_version).is_gt();
                    let winner = if remote_wins {
                        remote_version
                    } else {
                        local_version
                    };
                    let merged = local_version.merge_with_winner(remote_version, winner);
                    if !remote_wins {
                        set_entity_version_locked(&tx, "clipboard", &remote.id, &merged, false)?;
                        tx.commit()?;
                        return Ok(false);
                    }
                    merged
                }
            }
        } else {
            remote_version.clone()
        };
        upsert_remote_clipboard_locked(&tx, remote)?;
        set_entity_version_locked(&tx, "clipboard", &remote.id, &version_to_store, false)?;
        tx.commit()?;
        Ok(true)
    }

    #[allow(dead_code)]
    pub fn get_notes_since(&self, since: &str) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, content, is_pinned, created_at, updated_at, version, is_deleted
             FROM notes WHERE updated_at > ?1",
        )?;

        let notes = stmt
            .query_map(params![since], |row| {
                Ok(Note {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    is_pinned: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    version: row.get(6)?,
                    is_deleted: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        Ok(notes)
    }
}

fn get_note_locked(conn: &Connection, id: &str, include_deleted: bool) -> Result<Option<Note>> {
    conn.query_row(
        "SELECT id, title, content, is_pinned, created_at, updated_at, version, is_deleted
         FROM notes WHERE id = ?1 AND (?2 = 1 OR is_deleted = 0)",
        params![id, include_deleted],
        |row| {
            Ok(Note {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                is_pinned: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                version: row.get(6)?,
                is_deleted: row.get(7)?,
            })
        },
    )
    .optional()
}

fn attachment_from_row(row: &rusqlite::Row<'_>) -> Result<AttachmentRecord> {
    Ok(AttachmentRecord {
        id: row.get(0)?,
        relative_path: row.get(1)?,
        mime_type: row.get(2)?,
        size: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn clipboard_item_from_row(row: &rusqlite::Row<'_>) -> Result<ClipboardItem> {
    Ok(ClipboardItem {
        id: row.get(0)?,
        kind: row.get(1)?,
        content: row.get(2)?,
        preview: row.get(3)?,
        source_device: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        last_copied_at: row.get(7)?,
        capture_count: row.get(8)?,
        is_pinned: row.get(9)?,
        is_deleted: row.get(10)?,
    })
}

fn get_clipboard_item_locked(
    conn: &Connection,
    id: &str,
    include_deleted: bool,
) -> Result<Option<ClipboardItem>> {
    conn.query_row(
        "SELECT id, kind, content, preview, source_device, created_at, updated_at,
                last_copied_at, capture_count, is_pinned, is_deleted
         FROM clipboard_items WHERE id = ?1 AND (?2 = 1 OR is_deleted = 0)",
        params![id, include_deleted],
        clipboard_item_from_row,
    )
    .optional()
}

fn upsert_remote_clipboard_locked(conn: &Connection, item: &ClipboardItem) -> Result<()> {
    conn.execute(
        "INSERT INTO clipboard_items(
            id, kind, content, preview, source_device, created_at, updated_at,
            last_copied_at, capture_count, is_pinned, is_deleted
         ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(id) DO UPDATE SET
            kind = excluded.kind,
            content = excluded.content,
            preview = excluded.preview,
            source_device = excluded.source_device,
            updated_at = excluded.updated_at,
            last_copied_at = excluded.last_copied_at,
            capture_count = excluded.capture_count,
            is_pinned = excluded.is_pinned,
            is_deleted = excluded.is_deleted",
        params![
            item.id,
            item.kind,
            item.content,
            item.preview,
            item.source_device,
            item.created_at,
            item.updated_at,
            item.last_copied_at,
            item.capture_count,
            item.is_pinned,
            item.is_deleted,
        ],
    )?;
    Ok(())
}

fn normalize_clipboard_content(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

fn classify_clipboard_content(content: &str) -> String {
    let trimmed = content.trim();
    if (trimmed.starts_with("https://") || trimmed.starts_with("http://"))
        && !trimmed.chars().any(char::is_whitespace)
    {
        "link".to_string()
    } else if content.contains('\n')
        && ["{", "};", "=>", "def ", "fn ", "class ", "import "]
            .iter()
            .any(|needle| content.contains(needle))
    {
        "code".to_string()
    } else {
        "text".to_string()
    }
}

fn get_entity_version_locked(
    conn: &Connection,
    entity_type: &str,
    entity_id: &str,
) -> Result<Option<CausalVersion>> {
    let raw = conn
        .query_row(
            "SELECT version_json FROM sync_entity_versions
             WHERE entity_type = ?1 AND entity_id = ?2",
            params![entity_type, entity_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    raw.map(|value| {
        serde_json::from_str(&value).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
    })
    .transpose()
}

fn set_entity_version_locked(
    conn: &Connection,
    entity_type: &str,
    entity_id: &str,
    version: &CausalVersion,
    dirty: bool,
) -> Result<()> {
    let version_json = serde_json::to_string(version)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    conn.execute(
        "INSERT INTO sync_entity_versions(entity_type, entity_id, version_json, dirty)
         VALUES(?1, ?2, ?3, ?4)
         ON CONFLICT(entity_type, entity_id) DO UPDATE SET
            version_json = excluded.version_json,
            dirty = excluded.dirty",
        params![entity_type, entity_id, version_json, dirty],
    )?;
    Ok(())
}

fn ensure_local_causal_version_locked(
    conn: &Connection,
    entity_type: &str,
    entity_id: &str,
    device_id: &str,
) -> Result<CausalVersion> {
    let row = conn
        .query_row(
            "SELECT version_json, dirty FROM sync_entity_versions
             WHERE entity_type = ?1 AND entity_id = ?2",
            params![entity_type, entity_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
        )
        .optional()?;
    let (mut version, dirty) = match row {
        Some((raw, dirty)) => (
            serde_json::from_str(&raw).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
            dirty,
        ),
        None => (CausalVersion::default(), true),
    };
    if dirty {
        version.increment(device_id);
        set_entity_version_locked(conn, entity_type, entity_id, &version, false)?;
    }
    Ok(version)
}

fn upsert_remote_note_locked(conn: &Connection, remote: &Note) -> Result<()> {
    conn.execute(
        "INSERT INTO notes(
            id, title, content, plain_text, preview, is_pinned,
            created_at, updated_at, version, is_deleted
         ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            content = excluded.content,
            plain_text = excluded.plain_text,
            preview = excluded.preview,
            is_pinned = excluded.is_pinned,
            updated_at = excluded.updated_at,
            version = excluded.version,
            is_deleted = excluded.is_deleted",
        params![
            remote.id,
            remote.title,
            remote.content,
            html_to_text(&remote.content),
            make_preview_without_title(&remote.content),
            remote.is_pinned,
            remote.created_at,
            remote.updated_at,
            remote.version,
            remote.is_deleted,
        ],
    )?;
    Ok(())
}

fn enqueue_change(
    conn: &Connection,
    entity_type: &str,
    entity_id: &str,
    operation: &str,
    changed_at: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO sync_changes(entity_type, entity_id, operation, changed_at, synced)
         VALUES(?1, ?2, ?3, ?4, 0)",
        params![entity_type, entity_id, operation, changed_at],
    )?;
    conn.execute(
        "INSERT INTO sync_entity_versions(entity_type, entity_id, version_json, dirty)
         VALUES(?1, ?2, '{}', 1)
         ON CONFLICT(entity_type, entity_id) DO UPDATE SET dirty = 1",
        params![entity_type, entity_id],
    )?;
    Ok(())
}

fn insert_conflict_copy(
    conn: &Connection,
    source: &Note,
    source_version: &CausalVersion,
) -> Result<String> {
    let version_json = serde_json::to_string(source_version)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let digest = format!(
        "{:x}",
        Sha256::digest(format!("{}:{version_json}", source.id))
    );
    let id = format!("conflict-{}", &digest[..32]);
    let now = Utc::now().to_rfc3339();
    let title = format!("{} (冲突副本)", source.title);
    let plain_text = html_to_text(&source.content);
    let preview = make_preview_without_title(&source.content);
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO notes(
            id, title, content, plain_text, preview, is_pinned,
            created_at, updated_at, version, is_deleted
         ) VALUES(?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, 1, 0)",
        params![id, title, source.content, plain_text, preview, now, now],
    )?;
    if inserted > 0 {
        enqueue_change(conn, "note", &id, "upsert", &now)?;
    }
    Ok(id)
}

fn extract_title(content: &str) -> String {
    let plain_text = html_to_text(content);
    let title = plain_text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim_start_matches('#')
        .trim();

    if title.is_empty() {
        "无标题".to_string()
    } else {
        truncate_chars(title, 100)
    }
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>>>()?;

    if !columns.iter().any(|name| name == column) {
        conn.execute(
            &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, definition),
            [],
        )?;
    }

    Ok(())
}

fn make_preview_without_title(content: &str) -> String {
    let text = html_to_text(content);
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let _title = lines.next();
    let body = lines.collect::<Vec<_>>().join(" ");

    if body.is_empty() {
        String::new()
    } else {
        truncate_chars(&body, 200)
    }
}

fn prune_unpinned_versions(conn: &Connection, note_id: &str, limit: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM note_versions
         WHERE note_id = ?1
           AND is_pinned = 0
           AND id NOT IN (
             SELECT id FROM note_versions
             WHERE note_id = ?1 AND is_pinned = 0
             ORDER BY created_at DESC, id DESC
             LIMIT ?2
           )",
        params![note_id, limit],
    )?;
    Ok(())
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn normalize_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .filter_map(|term| {
            let cleaned = term.trim_matches(|c: char| c.is_ascii_punctuation() && c != '_');
            if cleaned.is_empty() {
                None
            } else {
                Some(format!("\"{}\"*", cleaned.replace('"', "\"\"")))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn html_to_text(content: &str) -> String {
    let mut text = String::with_capacity(content.len());
    let mut in_tag = false;
    let mut last_was_space = false;
    let mut tag_buf = String::new();

    for ch in content.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag_buf.clear();
                push_space(&mut text, &mut last_was_space);
            }
            '>' if in_tag => {
                in_tag = false;
                let tag = tag_buf.trim().to_ascii_lowercase();
                if tag.starts_with("img") {
                    push_word(&mut text, &mut last_was_space, "[图片]");
                } else if tag.starts_with("br")
                    || tag.starts_with("/p")
                    || tag.starts_with("/h")
                    || tag.starts_with("/li")
                    || tag.starts_with("/div")
                {
                    push_line_break(&mut text, &mut last_was_space);
                }
            }
            _ if in_tag => tag_buf.push(ch),
            c if c.is_whitespace() => push_space(&mut text, &mut last_was_space),
            c => {
                text.push(c);
                last_was_space = false;
            }
        }
    }

    decode_basic_entities(text.trim()).to_string()
}

fn push_space(text: &mut String, last_was_space: &mut bool) {
    if !*last_was_space && !text.is_empty() {
        text.push(' ');
        *last_was_space = true;
    }
}

fn push_word(text: &mut String, last_was_space: &mut bool, word: &str) {
    push_space(text, last_was_space);
    text.push_str(word);
    *last_was_space = false;
}

fn push_line_break(text: &mut String, last_was_space: &mut bool) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
        *last_was_space = true;
    }
}

fn decode_basic_entities(value: &str) -> String {
    value
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        (dir, db)
    }

    #[test]
    fn note_mutation_and_outbox_commit_together() {
        let (_dir, db) = database();
        let note = db.create_note("<p>标题</p><p>正文</p>").unwrap();
        db.update_note(&note.id, "<p>新标题</p><p>新正文</p>")
            .unwrap();

        let loaded = db.get_note(&note.id).unwrap().unwrap();
        assert_eq!(loaded.title, "新标题");
        assert_eq!(db.list_notes().unwrap()[0].preview, "新正文");
        assert!(db.list_pending_changes(10).unwrap().len() >= 2);
    }

    #[test]
    fn invalid_version_restore_has_no_side_effect() {
        let (_dir, db) = database();
        let note = db.create_note("<p>标题</p><p>正文</p>").unwrap();
        let before = db.get_note_versions(&note.id).unwrap().len();

        assert!(db
            .restore_note_version(&note.id, i64::MAX)
            .unwrap()
            .is_none());
        assert_eq!(db.get_note_versions(&note.id).unwrap().len(), before);
    }

    #[test]
    fn remote_change_preserves_dirty_local_content_as_conflict_copy() {
        let (_dir, db) = database();
        let local = db.create_note("<p>本地标题</p><p>本地正文</p>").unwrap();
        let remote = Note {
            id: local.id.clone(),
            title: "远端标题".to_string(),
            content: "<p>远端标题</p><p>远端正文</p>".to_string(),
            is_pinned: false,
            created_at: local.created_at,
            updated_at: "9999-12-31T23:59:59Z".to_string(),
            version: 2,
            is_deleted: false,
        };

        let remote_version = CausalVersion::legacy("device-z", 1);
        let (changed, conflict) = db
            .apply_remote_note(&remote, &remote_version, "device-a")
            .unwrap();
        assert!(changed);
        assert!(conflict);
        let notes = db.list_notes().unwrap();
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().any(|note| note.title.contains("冲突副本")));
    }

    #[test]
    fn causal_versions_detect_dominance_and_concurrency() {
        let base = CausalVersion::legacy("seed", 1);
        let mut left = base.clone();
        left.increment("device-a");
        let mut right = base.clone();
        right.increment("device-b");
        assert_eq!(left.relation(&base), CausalRelation::Dominates);
        assert_eq!(base.relation(&left), CausalRelation::Dominated);
        assert_eq!(left.relation(&right), CausalRelation::Concurrent);
    }

    #[test]
    fn concurrent_edits_converge_without_using_wall_clock() {
        let (_dir_a, db_a) = database();
        let (_dir_b, db_b) = database();
        let seed_version = CausalVersion::legacy("seed", 1);
        let seed = Note {
            id: "shared-note".to_string(),
            title: "Seed".to_string(),
            content: "<p>Seed</p>".to_string(),
            is_pinned: false,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            version: 1,
            is_deleted: false,
        };
        db_a.apply_remote_note(&seed, &seed_version, "device-a")
            .unwrap();
        db_b.apply_remote_note(&seed, &seed_version, "device-b")
            .unwrap();

        db_a.update_note(&seed.id, "<p>Edit A</p>").unwrap();
        db_b.update_note(&seed.id, "<p>Edit B</p>").unwrap();
        let version_a = db_a
            .ensure_local_causal_version("note", &seed.id, "device-a")
            .unwrap();
        let version_b = db_b
            .ensure_local_causal_version("note", &seed.id, "device-b")
            .unwrap();
        let mut note_a = db_a.get_note(&seed.id).unwrap().unwrap();
        let mut note_b = db_b.get_note(&seed.id).unwrap().unwrap();
        note_a.updated_at = "9999-12-31T23:59:59Z".to_string();
        note_b.updated_at = "1900-01-01T00:00:00Z".to_string();

        let (_, conflict_a) = db_a
            .apply_remote_note(&note_b, &version_b, "device-a")
            .unwrap();
        let (_, conflict_b) = db_b
            .apply_remote_note(&note_a, &version_a, "device-b")
            .unwrap();
        assert!(conflict_a && conflict_b);
        assert_eq!(
            db_a.get_note(&seed.id).unwrap().unwrap().content,
            "<p>Edit B</p>"
        );
        assert_eq!(
            db_b.get_note(&seed.id).unwrap().unwrap().content,
            "<p>Edit B</p>"
        );
        assert_eq!(db_a.list_notes().unwrap().len(), 2);
        assert_eq!(db_b.list_notes().unwrap().len(), 2);
    }

    #[test]
    fn concurrent_delete_and_edit_converge_and_preserve_the_edit() {
        let (_dir_a, db_a) = database();
        let (_dir_b, db_b) = database();
        let seed_version = CausalVersion::legacy("seed", 1);
        let seed = Note {
            id: "delete-race".to_string(),
            title: "Seed".to_string(),
            content: "<p>Seed</p>".to_string(),
            is_pinned: false,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            version: 1,
            is_deleted: false,
        };
        db_a.apply_remote_note(&seed, &seed_version, "device-a")
            .unwrap();
        db_b.apply_remote_note(&seed, &seed_version, "device-b")
            .unwrap();
        db_a.update_note(&seed.id, "<p>Keep this edit</p>").unwrap();
        db_b.delete_note(&seed.id).unwrap();
        let version_a = db_a
            .ensure_local_causal_version("note", &seed.id, "device-a")
            .unwrap();
        let version_b = db_b
            .ensure_local_causal_version("note", &seed.id, "device-b")
            .unwrap();
        let mut edited = db_a.get_note(&seed.id).unwrap().unwrap();
        edited.updated_at = "9999-12-31T23:59:59Z".to_string();

        let (_, conflict_a) = db_a
            .apply_remote_delete(&seed.id, "1900-01-01T00:00:00Z", &version_b, "device-a")
            .unwrap();
        let (_, conflict_b) = db_b
            .apply_remote_note(&edited, &version_a, "device-b")
            .unwrap();
        assert!(conflict_a && conflict_b);
        assert!(
            db_a.get_note_for_sync(&seed.id)
                .unwrap()
                .unwrap()
                .is_deleted
        );
        assert!(
            db_b.get_note_for_sync(&seed.id)
                .unwrap()
                .unwrap()
                .is_deleted
        );
        assert!(db_a
            .list_notes()
            .unwrap()
            .iter()
            .any(|note| note.title.contains("冲突副本")));
        assert!(db_b
            .list_notes()
            .unwrap()
            .iter()
            .any(|note| note.title.contains("冲突副本")));
    }

    #[test]
    fn clipboard_capture_deduplicates_classifies_and_queues_sync() {
        let (_dir, db) = database();
        let first = db
            .capture_clipboard("https://example.com/path\r\n", "device-a")
            .unwrap();
        let second = db
            .capture_clipboard("https://example.com/path\n", "device-a")
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(second.kind, "link");
        assert_eq!(second.capture_count, 2);
        assert_eq!(db.list_clipboard_items("example", 10).unwrap().len(), 1);
        assert!(db
            .list_pending_changes(20)
            .unwrap()
            .iter()
            .any(|change| change.entity_type == "clipboard"));
    }

    #[test]
    fn concurrent_clipboard_metadata_converges_by_causal_version() {
        let (_dir_a, db_a) = database();
        let (_dir_b, db_b) = database();
        let item_a = db_a.capture_clipboard("shared", "device-a").unwrap();
        let base_version = db_a
            .ensure_local_causal_version("clipboard", &item_a.id, "seed")
            .unwrap();
        db_b.apply_remote_clipboard(&item_a, &base_version, "device-b")
            .unwrap();
        db_a.toggle_clipboard_pin(&item_a.id).unwrap();
        db_b.delete_clipboard_item(&item_a.id).unwrap();
        let version_a = db_a
            .ensure_local_causal_version("clipboard", &item_a.id, "device-a")
            .unwrap();
        let version_b = db_b
            .ensure_local_causal_version("clipboard", &item_a.id, "device-b")
            .unwrap();
        let item_a = db_a
            .get_clipboard_item_for_sync(&item_a.id)
            .unwrap()
            .unwrap();
        let item_b = db_b
            .get_clipboard_item_for_sync(&item_a.id)
            .unwrap()
            .unwrap();
        db_a.apply_remote_clipboard(&item_b, &version_b, "device-a")
            .unwrap();
        db_b.apply_remote_clipboard(&item_a, &version_a, "device-b")
            .unwrap();
        assert_eq!(
            db_a.get_clipboard_item_for_sync(&item_a.id)
                .unwrap()
                .unwrap()
                .is_deleted,
            db_b.get_clipboard_item_for_sync(&item_a.id)
                .unwrap()
                .unwrap()
                .is_deleted
        );
    }
}
