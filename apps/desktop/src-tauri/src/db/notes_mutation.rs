use super::*;

impl Database {
    #[cfg(test)]
    pub fn update_note(&self, id: &str, content: &str) -> Result<Option<Note>> {
        self.update_note_with_yjs(id, content, None)
    }

    pub fn update_note_with_yjs(
        &self,
        id: &str,
        content: &str,
        yjs_state: Option<&[u8]>,
    ) -> Result<Option<Note>> {
        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let title = extract_title(content);
        let plain_text = html_to_text(content);
        let preview = make_preview_without_title(content);
        let tx = conn.transaction()?;

        self.snapshot_note_if_due_locked(&tx, id, &now)?;

        let rows = tx.execute(
            "UPDATE notes SET title = ?1, content = ?2, plain_text = ?3, preview = ?4,
                              updated_at = ?5, version = version + 1,
                              yjs_state = COALESCE(?7, yjs_state),
                              yjs_state_version = yjs_state_version + CASE WHEN ?7 IS NULL THEN 0 ELSE 1 END
             WHERE id = ?6 AND is_deleted = 0",
            params![title, content, plain_text, preview, now, id, yjs_state],
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

        let tag_ids = {
            let mut stmt = tx.prepare("SELECT tag_id FROM note_tags WHERE note_id = ?1")?;
            let rows = stmt
                .query_map(params![id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>>>()?;
            rows
        };

        let rows = tx.execute(
            "UPDATE notes SET is_deleted = 1, updated_at = ?1 WHERE id = ?2 AND is_deleted = 0",
            params![now, id],
        )?;
        if rows > 0 {
            enqueue_change(&tx, "note", id, "delete", &now)?;
            delete_unused_tags_locked(&tx, &tag_ids, &now)?;
        }
        tx.commit()?;

        Ok(rows > 0)
    }

    pub fn restore_note(&self, id: &str) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;

        let rows = tx.execute(
            "UPDATE notes SET is_deleted = 0, updated_at = ?1 WHERE id = ?2 AND is_deleted = 1",
            params![now, id],
        )?;
        if rows > 0 {
            enqueue_change(&tx, "note", id, "upsert", &now)?;
            restore_note_tags_locked(&tx, id, &now)?;
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
}
