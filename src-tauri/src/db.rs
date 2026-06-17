use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;
use chrono::Utc;

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
                is_pinned INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                is_deleted INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;

        ensure_column(&conn, "notes", "plain_text", "TEXT NOT NULL DEFAULT ''")?;
        {
            let mut stmt = conn.prepare("SELECT id, content FROM notes WHERE content != ''")?;
            let rows = stmt
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>>>()?;
            drop(stmt);

            for (id, content) in rows {
                conn.execute(
                    "UPDATE notes SET plain_text = ?1 WHERE id = ?2",
                    params![html_to_text(&content), id],
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
        ensure_column(&conn, "note_versions", "is_pinned", "INTEGER NOT NULL DEFAULT 0")?;

        conn.execute_batch(
            "DROP TRIGGER IF EXISTS notes_ai;
             DROP TRIGGER IF EXISTS notes_ad;
             DROP TRIGGER IF EXISTS notes_au;
             DROP TABLE IF EXISTS notes_fts;"
        )?;

        // Create FTS5 virtual table for full-text search
        conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
                title,
                plain_text,
                content='notes',
                content_rowid='rowid'
            );"
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

        conn.execute("INSERT INTO notes_fts(notes_fts) VALUES('rebuild')", [])?;

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
        let conn = self.conn.lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let title = extract_title(content);
        let plain_text = html_to_text(content);

        conn.execute(
            "INSERT INTO notes (id, title, content, plain_text, is_pinned, created_at, updated_at, version, is_deleted)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, 1, 0)",
            params![id, title, content, plain_text, now, now],
        )?;

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
             FROM notes WHERE id = ?1 AND is_deleted = 0"
        )?;

        let note = stmt.query_row(params![id], |row| {
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
        }).optional()?;

        Ok(note)
    }

    pub fn list_notes(&self) -> Result<Vec<NoteSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, content, is_pinned, created_at, updated_at
             FROM notes
             WHERE is_deleted = 0
             ORDER BY is_pinned DESC, updated_at DESC"
        )?;

        let notes = stmt.query_map([], |row| {
            Ok(NoteSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                preview: make_preview_without_title(&row.get::<_, String>(2)?),
                is_pinned: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;

        Ok(notes)
    }

    pub fn update_note(&self, id: &str, content: &str) -> Result<Option<Note>> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let title = extract_title(content);
        let plain_text = html_to_text(content);

        self.snapshot_note_if_due_locked(&conn, id, &now)?;

        let rows = conn.execute(
            "UPDATE notes SET title = ?1, content = ?2, plain_text = ?3, updated_at = ?4, version = version + 1
             WHERE id = ?5 AND is_deleted = 0",
            params![title, content, plain_text, now, id],
        )?;

        if rows == 0 {
            return Ok(None);
        }

        // Fetch updated note
        drop(conn);
        self.get_note(id)
    }

    pub fn delete_note(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();

        let rows = conn.execute(
            "UPDATE notes SET is_deleted = 1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;

        Ok(rows > 0)
    }

    pub fn restore_note(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();

        let rows = conn.execute(
            "UPDATE notes SET is_deleted = 0, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;

        Ok(rows > 0)
    }

    pub fn purge_note(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        conn.execute("DELETE FROM note_versions WHERE note_id = ?1", params![id])?;
        Ok(rows > 0)
    }

    pub fn list_deleted_notes(&self) -> Result<Vec<NoteSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, content, is_pinned, created_at, updated_at
             FROM notes
             WHERE is_deleted = 1
             ORDER BY updated_at DESC"
        )?;

        let notes = stmt.query_map([], |row| {
            Ok(NoteSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                preview: make_preview_without_title(&row.get::<_, String>(2)?),
                is_pinned: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;

        Ok(notes)
    }

    pub fn toggle_pin(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();

        let rows = conn.execute(
            "UPDATE notes SET is_pinned = NOT is_pinned, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;

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
            "SELECT n.id, n.title, n.content,
                    n.is_pinned, n.created_at, n.updated_at
             FROM notes n
             JOIN notes_fts fts ON n.rowid = fts.rowid
             WHERE notes_fts MATCH ?1 AND n.is_deleted = 0
             ORDER BY rank
             LIMIT 50"
        )?;

        let notes = stmt.query_map(params![normalized_query], |row| {
            Ok(NoteSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                preview: make_preview_without_title(&row.get::<_, String>(2)?),
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
             LIMIT 50"
        )?;

        let versions = stmt.query_map(params![id], |row| {
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
        let conn = self.conn.lock().unwrap();
        let current = conn
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
        conn.execute(
            "INSERT INTO note_versions (note_id, title, content, version, created_at, is_pinned)
             VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![id, title, content, version, now],
        )?;

        let version_content = conn
            .query_row(
                "SELECT content FROM note_versions WHERE id = ?1 AND note_id = ?2",
                params![version_id, note_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        let Some(content) = version_content else {
            return Ok(None);
        };

        let title = extract_title(&content);
        let plain_text = html_to_text(&content);
        conn.execute(
            "UPDATE notes SET title = ?1, content = ?2, plain_text = ?3, updated_at = ?4, version = version + 1
             WHERE id = ?5 AND is_deleted = 0",
            params![title, content, plain_text, now, note_id],
        )?;

        drop(conn);
        self.get_note(note_id)
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

    #[allow(dead_code)]
    pub fn get_notes_since(&self, since: &str) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, content, is_pinned, created_at, updated_at, version, is_deleted
             FROM notes WHERE updated_at > ?1"
        )?;

        let notes = stmt.query_map(params![since], |row| {
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
