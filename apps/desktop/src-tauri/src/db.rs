use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
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

#[derive(Default)]
pub struct DatabaseState {
    database: OnceLock<Arc<Database>>,
}

impl DatabaseState {
    pub fn initialize(&self, database: Arc<Database>) -> bool {
        self.database.set(database).is_ok()
    }

    pub fn arc(&self) -> Arc<Database> {
        self.database
            .get()
            .expect("database state has not been initialized")
            .clone()
    }
}

impl Deref for DatabaseState {
    type Target = Database;

    fn deref(&self) -> &Self::Target {
        self.database
            .get()
            .expect("database state has not been initialized")
    }
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
            CREATE TABLE IF NOT EXISTS clipboard_attachment_refs (
                clipboard_id TEXT NOT NULL,
                attachment_id TEXT NOT NULL,
                ordinal INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(clipboard_id, attachment_id)
            );
            CREATE TABLE IF NOT EXISTS attachment_gc_candidates (
                attachment_id TEXT PRIMARY KEY
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
            CREATE INDEX IF NOT EXISTS idx_clipboard_attachment_refs_attachment
                ON clipboard_attachment_refs(attachment_id);
            CREATE TRIGGER IF NOT EXISTS clipboard_attachment_ref_deleted
                AFTER DELETE ON clipboard_attachment_refs BEGIN
                    INSERT OR IGNORE INTO attachment_gc_candidates(attachment_id)
                    VALUES(old.attachment_id);
                END;
            CREATE TRIGGER IF NOT EXISTS clipboard_attachment_ref_inserted
                AFTER INSERT ON clipboard_attachment_refs BEGIN
                    DELETE FROM attachment_gc_candidates
                    WHERE attachment_id = new.attachment_id;
                END;
            CREATE INDEX IF NOT EXISTS idx_tags_normalized
                ON tags(normalized_name);
            CREATE INDEX IF NOT EXISTS idx_note_tags_tag
                ON note_tags(tag_id, note_id);
            CREATE INDEX IF NOT EXISTS idx_note_tags_note
                ON note_tags(note_id);",
        )?;

        if schema_version < 6 {
            let mut stmt = conn.prepare("SELECT id, content FROM clipboard_items")?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>>>()?;
            drop(stmt);
            for (clipboard_id, content) in rows {
                replace_clipboard_attachment_refs_locked(&conn, &clipboard_id, &content)?;
            }
        }

        conn.pragma_update(None, "user_version", 6)?;

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
}

mod causal;
mod clipboard;
mod clipboard_rows;
mod content;
mod note_rows;
mod notes_core;
mod notes_history;
mod notes_mutation;
mod sync_local;
mod sync_remote;
mod tags;
#[cfg(test)]
mod tests;
use causal::*;
use clipboard_rows::*;
use content::*;
use note_rows::*;

pub(crate) fn clipboard_attachment_ids(content: &str) -> Vec<String> {
    attachment_ids_from_content(content)
}
