use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

pub use quicknote_protocol::{
    AttachmentRecord, CausalRelation, CausalVersion, ClipboardItem, Note, NoteSummary, NoteTag,
    Tag, TagSummary,
};

const VERSION_SNAPSHOT_INTERVAL_SECONDS: i64 = 5 * 60;
const UNPINNED_VERSION_LIMIT: i64 = 10;

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
pub struct SyncChange {
    pub seq: i64,
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
    pub changed_at: String,
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
                yjs_state BLOB,
                yjs_state_version INTEGER NOT NULL DEFAULT 0,
                plain_text TEXT NOT NULL DEFAULT '',
                preview TEXT NOT NULL DEFAULT '',
                is_pinned INTEGER NOT NULL DEFAULT 0,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                is_deleted INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;

        ensure_column(&conn, "notes", "plain_text", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "notes", "preview", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "notes", "sort_order", "INTEGER NOT NULL DEFAULT 0")?;
        ensure_column(&conn, "notes", "yjs_state", "BLOB")?;
        ensure_column(
            &conn,
            "notes",
            "yjs_state_version",
            "INTEGER NOT NULL DEFAULT 0",
        )?;

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
            CREATE TABLE IF NOT EXISTS tags (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                normalized_name TEXT NOT NULL UNIQUE,
                color TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                is_deleted INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS note_tags (
                id TEXT PRIMARY KEY,
                note_id TEXT NOT NULL,
                tag_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(note_id, tag_id)
            );
            CREATE INDEX IF NOT EXISTS idx_sync_changes_pending
                ON sync_changes(synced, seq);
            CREATE INDEX IF NOT EXISTS idx_clipboard_recent
                ON clipboard_items(is_deleted, is_pinned DESC, last_copied_at DESC);
            CREATE INDEX IF NOT EXISTS idx_tags_normalized
                ON tags(normalized_name);
            CREATE INDEX IF NOT EXISTS idx_note_tags_tag
                ON note_tags(tag_id, note_id);
            CREATE INDEX IF NOT EXISTS idx_note_tags_note
                ON note_tags(note_id);",
        )?;

        conn.pragma_update(None, "user_version", 5)?;

        // Create index on updated_at for sync
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated_at)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_notes_pinned ON notes(is_pinned, sort_order DESC, updated_at DESC)",
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
        let sort_order = tx.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM notes WHERE is_deleted = 0 AND is_pinned = 0",
            [],
            |row| row.get::<_, i64>(0),
        )?;

        tx.execute(
            "INSERT INTO notes (id, title, content, plain_text, preview, is_pinned, sort_order, created_at, updated_at, version, is_deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, 1, 0)",
            params![id, title, content, plain_text, preview, sort_order, now, now],
        )?;
        enqueue_change(&tx, "note", &id, "upsert", &now)?;
        tx.commit()?;

        Ok(Note {
            id,
            title,
            content: content.to_string(),
            yjs_state: None,
            yjs_state_version: 0,
            is_pinned: false,
            sort_order,
            created_at: now.clone(),
            updated_at: now,
            version: 1,
            is_deleted: false,
            tags: Vec::new(),
        })
    }

    pub fn get_note(&self, id: &str) -> Result<Option<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, content, yjs_state, yjs_state_version, is_pinned, sort_order, created_at, updated_at, version, is_deleted
             FROM notes WHERE id = ?1 AND is_deleted = 0",
        )?;

        let note = stmt
            .query_row(params![id], |row| {
                Ok(Note {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    yjs_state: row.get(3)?,
                    yjs_state_version: row.get(4)?,
                    is_pinned: row.get(5)?,
                    sort_order: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                    version: row.get(9)?,
                    is_deleted: row.get(10)?,
                    tags: Vec::new(),
                })
            })
            .optional()?;

        Ok(note.map(|mut note| {
            note.tags = tags_for_note(&conn, &note.id).unwrap_or_default();
            note
        }))
    }

    pub fn list_notes(&self) -> Result<Vec<NoteSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, preview, is_pinned, created_at, updated_at
             FROM notes
             WHERE is_deleted = 0
             ORDER BY is_pinned DESC, sort_order DESC, updated_at DESC"
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
                    tags: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        attach_summary_tags(&conn, notes)
    }

    pub fn list_notes_by_tag(&self, normalized_name: &str) -> Result<Vec<NoteSummary>> {
        let conn = self.conn.lock().unwrap();
        let tag = normalize_tag_name(normalized_name);
        if tag.is_empty() {
            drop(conn);
            return self.list_notes();
        }
        let mut stmt = conn.prepare(
            "SELECT n.id, n.title, n.preview, n.is_pinned, n.created_at, n.updated_at
             FROM notes n
             JOIN note_tags nt ON nt.note_id = n.id
             JOIN tags t ON t.id = nt.tag_id
             WHERE n.is_deleted = 0 AND t.is_deleted = 0 AND t.normalized_name = ?1
             ORDER BY n.is_pinned DESC, n.sort_order DESC, n.updated_at DESC",
        )?;
        let notes = stmt
            .query_map(params![tag], |row| {
                Ok(NoteSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    preview: row.get(2)?,
                    is_pinned: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    tags: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>>>()?;
        attach_summary_tags(&conn, notes)
    }

    pub fn list_tags(&self) -> Result<Vec<TagSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.normalized_name, t.color, COUNT(n.id) AS note_count
             FROM tags t
             LEFT JOIN note_tags nt ON nt.tag_id = t.id
             LEFT JOIN notes n ON n.id = nt.note_id AND n.is_deleted = 0
             WHERE t.is_deleted = 0
             GROUP BY t.id, t.name, t.normalized_name, t.color
             ORDER BY note_count DESC, lower(t.name) ASC",
        )?;
        let tags = stmt.query_map([], |row| {
            Ok(TagSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                normalized_name: row.get(2)?,
                color: row.get(3)?,
                note_count: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
        Ok(tags)
    }

    pub fn set_note_tags(&self, note_id: &str, names: &[String]) -> Result<Option<Note>> {
        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;
        let note_exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM notes WHERE id = ?1 AND is_deleted = 0)",
            params![note_id],
            |row| row.get(0),
        )?;
        if !note_exists {
            tx.rollback()?;
            return Ok(None);
        }

        let normalized_names = normalize_tag_names(names);
        let mut next_tag_ids = Vec::new();
        for name in normalized_names {
            let normalized = normalize_tag_name(&name);
            if normalized.is_empty() {
                continue;
            }
            let existing: Option<String> = tx
                .query_row(
                    "SELECT id FROM tags WHERE normalized_name = ?1",
                    params![normalized],
                    |row| row.get(0),
                )
                .optional()?;
            let tag_id = existing.unwrap_or_else(|| tag_id_from_normalized(&normalized));
            tx.execute(
                "INSERT INTO tags(id, name, normalized_name, color, created_at, updated_at, is_deleted)
                 VALUES(?1, ?2, ?3, NULL, ?4, ?4, 0)
                 ON CONFLICT(normalized_name) DO UPDATE SET
                    name = excluded.name,
                    updated_at = excluded.updated_at,
                    is_deleted = 0",
                params![tag_id, name, normalized, now],
            )?;
            let stored_id: String = tx.query_row(
                "SELECT id FROM tags WHERE normalized_name = ?1",
                params![normalized],
                |row| row.get(0),
            )?;
            enqueue_change(&tx, "tag", &stored_id, "upsert", &now)?;
            next_tag_ids.push(stored_id);
        }

        let current_ids = {
            let mut stmt = tx.prepare("SELECT tag_id FROM note_tags WHERE note_id = ?1")?;
            let rows = stmt
                .query_map(params![note_id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>>>()?;
            rows
        };
        for tag_id in &current_ids {
            if !next_tag_ids.iter().any(|id| id == tag_id) {
                let relation_id = note_tag_id(note_id, tag_id);
                tx.execute("DELETE FROM note_tags WHERE id = ?1", params![relation_id])?;
                enqueue_change(&tx, "note_tag", &relation_id, "delete", &now)?;
            }
        }
        for tag_id in &next_tag_ids {
            let relation_id = note_tag_id(note_id, tag_id);
            let inserted = tx.execute(
                "INSERT OR IGNORE INTO note_tags(id, note_id, tag_id, created_at)
                 VALUES(?1, ?2, ?3, ?4)",
                params![relation_id, note_id, tag_id, now],
            )?;
            if inserted > 0 {
                enqueue_change(&tx, "note_tag", &relation_id, "upsert", &now)?;
            }
        }
        let note = get_note_locked(&tx, note_id, false)?;
        tx.commit()?;
        Ok(note)
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
                    tags: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        attach_summary_tags(&conn, notes)
    }

    pub fn toggle_pin(&self, id: &str) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;
        let is_pinned = tx
            .query_row(
                "SELECT is_pinned FROM notes WHERE id = ?1",
                params![id],
                |row| row.get::<_, bool>(0),
            )
            .optional()?
            .unwrap_or(false);
        let next_sort_order = tx.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM notes WHERE is_deleted = 0 AND is_pinned = ?1",
            params![!is_pinned],
            |row| row.get::<_, i64>(0),
        )?;

        let rows = tx.execute(
            "UPDATE notes SET is_pinned = NOT is_pinned, sort_order = ?1, updated_at = ?2 WHERE id = ?3",
            params![next_sort_order, now, id],
        )?;
        if rows > 0 {
            enqueue_change(&tx, "note", id, "upsert", &now)?;
        }
        tx.commit()?;

        Ok(rows > 0)
    }

    pub fn reorder_notes(&self, ids: &[String], is_pinned: bool) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;

        let len = ids.len() as i64;
        for (index, id) in ids.iter().enumerate() {
            let rows = tx.execute(
                "UPDATE notes SET is_pinned = ?1, sort_order = ?2, updated_at = ?3 WHERE id = ?4 AND is_deleted = 0",
                params![is_pinned, len - index as i64, now, id],
            )?;
            if rows > 0 {
                enqueue_change(&tx, "note", id, "upsert", &now)?;
            }
        }

        tx.commit()?;
        Ok(())
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
                    tags: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        attach_summary_tags(&conn, notes)
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

    pub fn delete_note_version(&self, version_id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "DELETE FROM note_versions WHERE id = ?1",
            params![version_id],
        )?;
        Ok(rows > 0)
    }

    pub fn clear_note_versions(&self, note_id: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "DELETE FROM note_versions WHERE note_id = ?1 AND is_pinned = 0",
            params![note_id],
        )?;
        Ok(rows)
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

    /// Read the current causal version for an entity without incrementing or marking dirty.
    pub fn get_entity_causal_version(
        &self,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Option<CausalVersion>> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                "SELECT version_json FROM sync_entity_versions
                 WHERE entity_type = ?1 AND entity_id = ?2",
                params![entity_type, entity_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match row {
            Some(raw) => {
                // Parse outside query_row to avoid error type mismatch
                match serde_json::from_str(&raw) {
                    Ok(version) => Ok(Some(version)),
                    Err(_) => Ok(None),
                }
            }
            None => Ok(None),
        }
    }

    /// Mark all pending changes for a specific entity as synced.
    pub fn mark_entity_changes_synced(
        &self,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sync_changes SET synced = 1
             WHERE entity_type = ?1 AND entity_id = ?2 AND synced = 0",
            params![entity_type, entity_id],
        )?;
        Ok(())
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
                .prepare(
                    "SELECT content FROM notes
                     UNION ALL SELECT content FROM note_versions
                     UNION ALL SELECT content FROM clipboard_items",
                )?;
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
        let now = Utc::now().to_rfc3339();
        self.capture_clipboard_at(content, source_device, &now)
    }

    pub fn capture_clipboard_at(
        &self,
        content: &str,
        source_device: &str,
        captured_at: &str,
    ) -> Result<ClipboardItem> {
        let normalized = normalize_clipboard_content(content);
        if normalized.trim().is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "clipboard content is empty".to_string(),
            ));
        }
        if normalized.len() > 5_000_000 {
            return Err(rusqlite::Error::InvalidParameterName(
                "clipboard content exceeds 5 MB".to_string(),
            ));
        }
        let id = format!(
            "{:x}",
            Sha256::digest(format!("clipboard:text:{normalized}"))
        );
        let kind = classify_clipboard_content(&normalized);
        let preview = clipboard_preview(&normalized, &kind);
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO clipboard_items(
                id, kind, content, preview, source_device, created_at, updated_at,
                last_copied_at, capture_count, is_pinned, is_deleted
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?6, ?6, 1, 0, 0)
             ON CONFLICT(id) DO UPDATE SET
                source_device = excluded.source_device,
                updated_at = CASE
                    WHEN excluded.last_copied_at > clipboard_items.last_copied_at THEN excluded.updated_at
                    ELSE clipboard_items.updated_at
                END,
                last_copied_at = CASE
                    WHEN excluded.last_copied_at > clipboard_items.last_copied_at THEN excluded.last_copied_at
                    ELSE clipboard_items.last_copied_at
                END,
                capture_count = CASE
                    WHEN excluded.last_copied_at > clipboard_items.last_copied_at THEN clipboard_items.capture_count + 1
                    ELSE clipboard_items.capture_count
                END,
                is_deleted = 0",
            params![id, kind, normalized, preview, source_device, captured_at],
        )?;
        enqueue_change(&tx, "clipboard", &id, "upsert", captured_at)?;
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
             ORDER BY is_pinned DESC, last_copied_at DESC, created_at DESC
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
            "DELETE FROM clipboard_items WHERE id = ?1",
            params![id],
        )?;
        if changed > 0 {
            enqueue_change(&tx, "clipboard", id, "delete", &now)?;
        }
        tx.commit()?;
        Ok(changed > 0)
    }

    pub fn clear_clipboard_items(&self) -> Result<usize> {
        let now = Utc::now().to_rfc3339();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let ids = {
            let mut stmt = tx.prepare("SELECT id FROM clipboard_items WHERE is_deleted = 0 AND is_pinned = 0")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>>>()?
        };

        let changed = tx.execute(
            "DELETE FROM clipboard_items WHERE is_pinned = 0",
            [],
        )?;

        if changed > 0 {
            for id in &ids {
                enqueue_change(&tx, "clipboard", id, "delete", &now)?;
            }
        }

        tx.commit()?;
        Ok(changed)
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

    pub fn get_tag_for_sync(&self, id: &str) -> Result<Option<Tag>> {
        let conn = self.conn.lock().unwrap();
        get_tag_locked(&conn, id, true)
    }

    pub fn get_note_tag_for_sync(&self, id: &str) -> Result<Option<NoteTag>> {
        let conn = self.conn.lock().unwrap();
        get_note_tag_locked(&conn, id)
    }

    pub fn apply_remote_tag(
        &self,
        remote: &Tag,
        remote_version: &CausalVersion,
        local_device_id: &str,
    ) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if should_skip_remote_entity(&tx, "tag", &remote.id, remote_version, local_device_id)? {
            tx.commit()?;
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO tags(id, name, normalized_name, color, created_at, updated_at, is_deleted)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                normalized_name = excluded.normalized_name,
                color = excluded.color,
                updated_at = excluded.updated_at,
                is_deleted = excluded.is_deleted",
            params![
                remote.id,
                remote.name,
                remote.normalized_name,
                remote.color,
                remote.created_at,
                remote.updated_at,
                remote.is_deleted,
            ],
        )?;
        set_entity_version_locked(&tx, "tag", &remote.id, remote_version, false)?;
        tx.commit()?;
        Ok(true)
    }

    pub fn apply_remote_note_tag(
        &self,
        remote: &NoteTag,
        remote_version: &CausalVersion,
        local_device_id: &str,
    ) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if should_skip_remote_entity(&tx, "note_tag", &remote.id, remote_version, local_device_id)?
        {
            tx.commit()?;
            return Ok(false);
        }
        tx.execute(
            "INSERT OR IGNORE INTO note_tags(id, note_id, tag_id, created_at)
             VALUES(?1, ?2, ?3, ?4)",
            params![remote.id, remote.note_id, remote.tag_id, remote.created_at],
        )?;
        set_entity_version_locked(&tx, "note_tag", &remote.id, remote_version, false)?;
        tx.commit()?;
        Ok(true)
    }

    pub fn apply_remote_note_tag_delete(
        &self,
        id: &str,
        remote_version: &CausalVersion,
        local_device_id: &str,
    ) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if should_skip_remote_entity(&tx, "note_tag", id, remote_version, local_device_id)? {
            tx.commit()?;
            return Ok(false);
        }
        let changed = tx.execute("DELETE FROM note_tags WHERE id = ?1", params![id])?;
        set_entity_version_locked(&tx, "note_tag", id, remote_version, false)?;
        tx.commit()?;
        Ok(changed > 0)
    }

    #[allow(dead_code)]
    pub fn get_notes_since(&self, since: &str) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, content, yjs_state, yjs_state_version, is_pinned, sort_order, created_at, updated_at, version, is_deleted
             FROM notes WHERE updated_at > ?1",
        )?;

        let notes = stmt
            .query_map(params![since], |row| {
                Ok(Note {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    yjs_state: row.get(3)?,
                    yjs_state_version: row.get(4)?,
                    is_pinned: row.get(5)?,
                    sort_order: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                    version: row.get(9)?,
                    is_deleted: row.get(10)?,
                    tags: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        Ok(notes
            .into_iter()
            .map(|mut note| {
                note.tags = tags_for_note(&conn, &note.id).unwrap_or_default();
                note
            })
            .collect())
    }
}

fn get_note_locked(conn: &Connection, id: &str, include_deleted: bool) -> Result<Option<Note>> {
    let note = conn.query_row(
        "SELECT id, title, content, yjs_state, yjs_state_version, is_pinned, sort_order, created_at, updated_at, version, is_deleted
         FROM notes WHERE id = ?1 AND (?2 = 1 OR is_deleted = 0)",
        params![id, include_deleted],
        |row| {
            Ok(Note {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                yjs_state: row.get(3)?,
                yjs_state_version: row.get(4)?,
                is_pinned: row.get(5)?,
                sort_order: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                version: row.get(9)?,
                is_deleted: row.get(10)?,
                tags: Vec::new(),
            })
        },
    )
    .optional()?;
    Ok(note.map(|mut note| {
        note.tags = tags_for_note(conn, &note.id).unwrap_or_default();
        note
    }))
}

fn get_tag_locked(conn: &Connection, id: &str, include_deleted: bool) -> Result<Option<Tag>> {
    conn.query_row(
        "SELECT id, name, normalized_name, color, created_at, updated_at, is_deleted
         FROM tags WHERE id = ?1 AND (?2 = 1 OR is_deleted = 0)",
        params![id, include_deleted],
        |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                normalized_name: row.get(2)?,
                color: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                is_deleted: row.get(6)?,
            })
        },
    )
    .optional()
}

fn get_note_tag_locked(conn: &Connection, id: &str) -> Result<Option<NoteTag>> {
    conn.query_row(
        "SELECT id, note_id, tag_id, created_at FROM note_tags WHERE id = ?1",
        params![id],
        |row| {
            Ok(NoteTag {
                id: row.get(0)?,
                note_id: row.get(1)?,
                tag_id: row.get(2)?,
                created_at: row.get(3)?,
            })
        },
    )
    .optional()
}

fn should_skip_remote_entity(
    conn: &Connection,
    entity_type: &str,
    entity_id: &str,
    remote_version: &CausalVersion,
    local_device_id: &str,
) -> Result<bool> {
    let local_dirty: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sync_changes
            WHERE synced = 0 AND entity_type = ?1 AND entity_id = ?2
        )",
        params![entity_type, entity_id],
        |row| row.get(0),
    )?;
    let mut local_version = get_entity_version_locked(conn, entity_type, entity_id)?;
    if local_dirty {
        local_version = Some(ensure_local_causal_version_locked(
            conn,
            entity_type,
            entity_id,
            local_device_id,
        )?);
    }
    if let Some(local_version) = &local_version {
        if matches!(
            local_version.relation(remote_version),
            CausalRelation::Equal | CausalRelation::Dominates
        ) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn attach_summary_tags(
    conn: &Connection,
    notes: Vec<NoteSummary>,
) -> Result<Vec<NoteSummary>> {
    notes
        .into_iter()
        .map(|mut note| {
            note.tags = tags_for_note(conn, &note.id)?;
            Ok(note)
        })
        .collect()
}

fn tags_for_note(conn: &Connection, note_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name
         FROM tags t
         JOIN note_tags nt ON nt.tag_id = t.id
         WHERE nt.note_id = ?1 AND t.is_deleted = 0
         ORDER BY lower(t.name) ASC",
    )?;
    let tags = stmt
        .query_map(params![note_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>>>()?;
    Ok(tags)
}

fn normalize_tag_names(names: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    names
        .iter()
        .filter_map(|name| {
            let display = name.trim().trim_start_matches('#').trim();
            let normalized = normalize_tag_name(display);
            if normalized.is_empty() || !seen.insert(normalized) {
                None
            } else {
                Some(display.chars().take(40).collect::<String>())
            }
        })
        .collect()
}

fn normalize_tag_name(name: &str) -> String {
    name.trim()
        .trim_start_matches('#')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn note_tag_id(note_id: &str, tag_id: &str) -> String {
    format!("{:x}", Sha256::digest(format!("{note_id}:{tag_id}")))
}

fn tag_id_from_normalized(normalized_name: &str) -> String {
    format!("{:x}", Sha256::digest(format!("tag:{normalized_name}")))
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
    if trimmed.starts_with("data:image/") || trimmed.starts_with("<img ") {
        "image".to_string()
    } else if looks_like_rich_clipboard(content) {
        "rich".to_string()
    } else if (trimmed.starts_with("https://") || trimmed.starts_with("http://"))
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

fn looks_like_rich_clipboard(content: &str) -> bool {
    let lowered = content.to_ascii_lowercase();
    lowered.contains("<img ")
        || lowered.contains("<table")
        || lowered.contains("<ul")
        || lowered.contains("<ol")
        || lowered.contains("<li")
        || lowered.contains("<pre")
        || lowered.contains("<code")
        || lowered.contains("<blockquote")
        || lowered.contains("<figure")
        || lowered.contains("<br")
        || count_html_block_tags(&lowered) > 1
}

fn count_html_block_tags(lowered: &str) -> usize {
    [
        "<p", "<div", "<section", "<article", "<h1", "<h2", "<h3", "<h4", "<h5", "<h6",
    ]
    .iter()
    .map(|needle| lowered.matches(needle).count())
    .sum()
}

fn clipboard_preview(content: &str, kind: &str) -> String {
    if kind == "image" {
        return "图片".to_string();
    }
    let preview = if kind == "rich" {
        strip_html_tags(content)
    } else {
        content.replace('\n', " ")
    };
    truncate_chars(&preview, 240)
}

fn strip_html_tags(content: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let mut in_tag = false;
    for ch in content.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                output.push(' ');
            }
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
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
            id, title, content, plain_text, preview, is_pinned, sort_order,
            created_at, updated_at, version, is_deleted
         ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            content = excluded.content,
            plain_text = excluded.plain_text,
            preview = excluded.preview,
            is_pinned = excluded.is_pinned,
            sort_order = excluded.sort_order,
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
            remote.sort_order,
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
            yjs_state: None,
            yjs_state_version: 0,
            is_pinned: false,
            sort_order: 0,
            created_at: local.created_at,
            updated_at: "9999-12-31T23:59:59Z".to_string(),
            version: 2,
            is_deleted: false,
            tags: Vec::new(),
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
            yjs_state: None,
            yjs_state_version: 0,
            is_pinned: false,
            sort_order: 0,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            version: 1,
            is_deleted: false,
            tags: Vec::new(),
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
            yjs_state: None,
            yjs_state_version: 0,
            is_pinned: false,
            sort_order: 0,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            version: 1,
            is_deleted: false,
            tags: Vec::new(),
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
    fn duplicate_clipboard_capture_moves_item_to_the_front_of_its_group() {
        let (_dir, db) = database();
        db.capture_clipboard_at("first", "device-a", "2026-01-01T00:00:00Z")
            .unwrap();
        db.capture_clipboard_at("second", "device-a", "2026-01-01T00:00:10Z")
            .unwrap();
        db.capture_clipboard_at("first", "device-a", "2026-01-01T00:00:20Z")
            .unwrap();

        let items = db.list_clipboard_items("", 10).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].content, "first");
        assert_eq!(items[0].capture_count, 2);
        assert_eq!(items[1].content, "second");
    }

    #[test]
    fn clipboard_image_attachments_are_not_orphaned() {
        let (_dir, db) = database();
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        db.register_attachment(id, &format!("{id}.webp"), "image/webp", 12)
            .unwrap();
        db.capture_clipboard(
            &format!(r#"<img src="attachment://{id}" alt="剪贴板图片">"#),
            "device-a",
        )
        .unwrap();

        assert!(db.orphan_attachments().unwrap().is_empty());
    }

    #[test]
    fn bootstrap_can_requeue_historical_data_for_a_new_cloud_scope() {
        let (_dir, db) = database();
        let note = db.create_note("<p>历史数据</p>").unwrap();

        for change in db.list_pending_changes(20).unwrap() {
            db.mark_change_synced(change.seq).unwrap();
        }
        assert!(db.list_pending_changes(20).unwrap().is_empty());

        db.ensure_sync_bootstrap("cloud:https://cloud.test:user@example.com")
            .unwrap();

        let pending = db.list_pending_changes(20).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].entity_type, "note");
        assert_eq!(pending[0].entity_id, note.id);

        db.ensure_sync_bootstrap("cloud:https://cloud.test:user@example.com")
            .unwrap();
        assert_eq!(db.list_pending_changes(20).unwrap().len(), 1);
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
        db_b.toggle_clipboard_pin(&item_a.id).unwrap();
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
                .is_pinned,
            db_b.get_clipboard_item_for_sync(&item_a.id)
                .unwrap()
                .unwrap()
                .is_pinned
        );
    }
}
