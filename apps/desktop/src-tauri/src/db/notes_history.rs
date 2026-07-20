use super::*;

impl Database {
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

    pub(super) fn snapshot_note_if_due_locked(
        &self,
        conn: &Connection,
        id: &str,
        now: &str,
    ) -> Result<()> {
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
}
