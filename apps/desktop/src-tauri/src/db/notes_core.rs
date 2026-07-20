use super::*;

impl Database {
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
             ORDER BY is_pinned DESC, sort_order DESC, updated_at DESC",
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
             HAVING COUNT(n.id) > 0
             ORDER BY note_count DESC, lower(t.name) ASC",
        )?;
        let tags = stmt
            .query_map([], |row| {
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
        delete_unused_tags_locked(&tx, &current_ids, &now)?;
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
}
